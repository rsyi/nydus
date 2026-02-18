use crate::config::{Instance, Profile};
use crate::error::{NydusError, Result};
use crate::util;
use aws_sdk_ec2::types::{BlockDeviceMapping, EbsBlockDevice, Filter, InstanceStateName, ResourceType, Tag, TagSpecification, VolumeType};
use aws_sdk_ec2::Client;
use colored::*;
use std::time::Instant;

/// Initialize an EC2 client for the specified region
pub async fn initialize_ec2_client(region: &str) -> Result<Client> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;

    Ok(Client::new(&config))
}

/// Run (create) a new EC2 instance
pub async fn run_instance(profile: &Profile, name: &str) -> Result<Instance> {
    let client = initialize_ec2_client(&profile.region).await?;

    // Resolve AMI
    let ami_id = if let Some(ami) = &profile.ami {
        ami.clone()
    } else {
        resolve_latest_ubuntu_ami(&client).await?
    };

    // Ensure security group exists
    let security_group_id = ensure_security_group(&client, name).await?;

    // Expand SSH key path
    let ssh_key_path = util::expand_tilde(&profile.ssh_key_path)?
        .to_string_lossy()
        .to_string();

    // Extract key name from path (needed for EC2)
    let key_name = std::path::Path::new(&ssh_key_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| NydusError::ConfigError("Invalid SSH key path".to_string()))?;

    // Build tags
    let mut tags = vec![
        Tag::builder().key("Name").value(name).build(),
    ];

    // Add ManagedBy tag only if not already in profile tags
    if !profile.tags.contains_key("ManagedBy") {
        tags.push(Tag::builder()
            .key("ManagedBy")
            .value("nydus")
            .build());
    }

    // Add nydus metadata tags for state reconstruction
    tags.push(Tag::builder().key("nydus:profile").value(&profile.name).build());
    tags.push(Tag::builder().key("nydus:ssh-user").value(&profile.ssh_user).build());
    tags.push(Tag::builder().key("nydus:ssh-key-path").value(&profile.ssh_key_path).build());
    tags.push(Tag::builder().key("nydus:created-at").value(chrono::Utc::now().timestamp().to_string()).build());

    for (key, value) in &profile.tags {
        tags.push(Tag::builder().key(key).value(value).build());
    }

    let tag_spec = TagSpecification::builder()
        .resource_type(ResourceType::Instance)
        .set_tags(Some(tags))
        .build();

    // Configure root volume size
    let block_device = BlockDeviceMapping::builder()
        .device_name("/dev/sda1")
        .ebs(
            EbsBlockDevice::builder()
                .volume_size(profile.volume_size_gb as i32)
                .volume_type(VolumeType::Gp3)
                .delete_on_termination(true)
                .build(),
        )
        .build();

    // Launch instance
    let run_result = client
        .run_instances()
        .image_id(&ami_id)
        .instance_type(
            profile
                .instance_type
                .parse()
                .map_err(|e| NydusError::AwsError(format!("Invalid instance type: {}", e)))?,
        )
        .key_name(key_name)
        .max_count(1)
        .min_count(1)
        .set_security_group_ids(Some(vec![security_group_id]))
        .set_subnet_id(profile.subnet_id.clone())
        .block_device_mappings(block_device)
        .tag_specifications(tag_spec)
        .send()
        .await
        .map_err(|e| {
            let err_msg = format!("{:?}", e);
            NydusError::AwsError(format!("Failed to launch instance: {}", err_msg))
        })?;

    let ec2_instance = run_result
        .instances()
        .first()
        .ok_or_else(|| NydusError::AwsError("No instance returned from RunInstances".to_string()))?;

    let instance_id = ec2_instance
        .instance_id()
        .ok_or_else(|| NydusError::AwsError("Instance ID not available".to_string()))?
        .to_string();

    // Wait for instance to be running
    println!("Waiting for instance to be running...");
    wait_for_instance_running(&client, &instance_id).await?;

    // Refresh instance details
    let instance = describe_instance(&client, &instance_id, name, profile).await?;

    // Wait for SSH to be ready with retries
    println!("Waiting for SSH to be ready...");
    wait_for_ssh_ready(&instance).await?;

    Ok(instance)
}

/// Start a stopped instance
pub async fn start_instance(instance: &Instance) -> Result<()> {
    let client = initialize_ec2_client(&instance.region).await?;

    client
        .start_instances()
        .instance_ids(&instance.instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to start instance: {}", e)))?;

    println!("Starting instance {}...", instance.name);
    wait_for_instance_running(&client, &instance.instance_id).await?;

    Ok(())
}

/// Stop a running instance
pub async fn stop_instance(instance: &Instance) -> Result<()> {
    let client = initialize_ec2_client(&instance.region).await?;

    client
        .stop_instances()
        .instance_ids(&instance.instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to stop instance: {}", e)))?;

    println!("Stopping instance {}...", instance.name);

    Ok(())
}

/// Terminate an instance
pub async fn terminate_instance(instance: &Instance) -> Result<()> {
    let client = initialize_ec2_client(&instance.region).await?;

    client
        .terminate_instances()
        .instance_ids(&instance.instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to terminate instance: {}", e)))?;

    println!("Terminating instance {}...", instance.name);

    Ok(())
}

/// Describe (refresh) instance details
pub async fn describe_instance(
    client: &Client,
    instance_id: &str,
    name: &str,
    profile: &Profile,
) -> Result<Instance> {
    let result = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe instance: {}", e)))?;

    let ec2_instance = result
        .reservations()
        .first()
        .and_then(|r| r.instances().first())
        .ok_or_else(|| NydusError::InstanceNotFound(instance_id.to_string()))?;

    let status = ec2_instance
        .state()
        .and_then(|s| s.name())
        .map(|n| format!("{:?}", n))
        .unwrap_or_else(|| "unknown".to_string());

    let ssh_key_path = util::expand_tilde(&profile.ssh_key_path)?
        .to_string_lossy()
        .to_string();

    Ok(Instance {
        id: None,
        name: name.to_string(),
        profile: profile.name.clone(),
        region: profile.region.clone(),
        instance_id: instance_id.to_string(),
        public_ip: ec2_instance.public_ip_address().map(|s| s.to_string()),
        public_dns: ec2_instance.public_dns_name().map(|s| s.to_string()),
        private_ip: ec2_instance.private_ip_address().map(|s| s.to_string()),
        ssh_user: profile.ssh_user.clone(),
        ssh_key_path,
        created_at: util::current_timestamp(),
        last_seen: util::current_timestamp(),
        last_synced: None,
        desired_state: "running".to_string(),
        status: Some(status),
        notes: None,
    })
}

/// Refresh instance from EC2 by instance struct
pub async fn refresh_instance(instance: &Instance) -> Result<Instance> {
    let client = initialize_ec2_client(&instance.region).await?;

    let result = client
        .describe_instances()
        .instance_ids(&instance.instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe instance: {}", e)))?;

    let ec2_instance = result
        .reservations()
        .first()
        .and_then(|r| r.instances().first())
        .ok_or_else(|| NydusError::InstanceNotFound(instance.instance_id.clone()))?;

    let status = ec2_instance
        .state()
        .and_then(|s| s.name())
        .map(|n| format!("{:?}", n))
        .unwrap_or_else(|| "unknown".to_string());

    let mut updated = instance.clone();
    updated.public_ip = ec2_instance.public_ip_address().map(|s| s.to_string());
    updated.public_dns = ec2_instance.public_dns_name().map(|s| s.to_string());
    updated.private_ip = ec2_instance.private_ip_address().map(|s| s.to_string());
    updated.last_seen = util::current_timestamp();
    updated.status = Some(status);

    Ok(updated)
}

/// Wait for instance to reach running state
async fn wait_for_instance_running(client: &Client, instance_id: &str) -> Result<()> {
    use std::sync::Arc;

    let start_time = Arc::new(Instant::now());

    print!("⏱  Waiting for instance to start... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    // Spawn timer update task
    let timer_start = start_time.clone();
    let timer_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let elapsed = timer_start.elapsed();
            let elapsed_str = format!("{:.1}s", elapsed.as_secs_f64());
            print!("\r⏱  Waiting for instance to start... {}     ", elapsed_str.truecolor(255, 165, 0).bold());
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
    });

    // Check instance state
    let client_clone = client.clone();
    let instance_id_clone = instance_id.to_string();
    let check_start = start_time.clone();
    let check_task = tokio::spawn(async move {
        let mut not_found_retries = 0;
        let max_not_found_retries = 10;

        loop {
            if check_start.elapsed() >= tokio::time::Duration::from_secs(300) {
                return Err(NydusError::AwsError("Instance did not start within 5 minutes".to_string()));
            }

            let result = client_clone
                .describe_instances()
                .instance_ids(&instance_id_clone)
                .send()
                .await;

            match result {
                Err(e) => {
                    let error_msg = format!("{:?}", e);
                    if error_msg.contains("InvalidInstanceID.NotFound") && not_found_retries < max_not_found_retries {
                        not_found_retries += 1;
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        continue;
                    } else {
                        return Err(NydusError::AwsError(format!(
                            "Failed to describe instance '{}': {:?}\n\
                             This could mean:\n\
                             - IAM permissions issue (check DescribeInstances permission)\n\
                             - Instance was created in a different region\n\
                             - Instance ID is invalid",
                            instance_id_clone, e
                        )));
                    }
                }
                Ok(result) => {
                    let state = result
                        .reservations()
                        .first()
                        .and_then(|r| r.instances().first())
                        .and_then(|i| i.state())
                        .and_then(|s| s.name());

                    match state {
                        Some(InstanceStateName::Running) => {
                            return Ok(());
                        }
                        Some(InstanceStateName::Terminated) | Some(InstanceStateName::ShuttingDown) => {
                            return Err(NydusError::AwsError("Instance terminated".to_string()));
                        }
                        _ => {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        }
                    }
                }
            }
        }
    });

    // Wait for check to complete
    let result = check_task.await.map_err(|e| NydusError::AwsError(format!("Task error: {}", e)))?;

    // Give the timer a moment to catch up and display the final time
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Calculate final elapsed time before stopping timer
    let final_elapsed = start_time.elapsed();

    // Stop the timer task
    timer_task.abort();

    // Clear line and show final message
    print!("\r\x1b[2K");

    match result {
        Ok(_) => {
            let final_time = format!("{:.1}s", final_elapsed.as_secs_f64());
            print!("✓ Instance is running! ");
            println!("{}", format!("({})", final_time).dimmed());
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Wait for SSH to be ready on the instance
async fn wait_for_ssh_ready(instance: &Instance) -> Result<()> {
    use std::sync::Arc;

    let max_duration = tokio::time::Duration::from_secs(180);
    let start_time = Arc::new(Instant::now());

    print!("⏱  Waiting for SSH to be ready... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    // Spawn timer update task
    let timer_start = start_time.clone();
    let timer_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let elapsed = timer_start.elapsed();
            let elapsed_str = format!("{:.1}s", elapsed.as_secs_f64());
            print!("\r⏱  Waiting for SSH to be ready... {}     ", elapsed_str.truecolor(255, 165, 0).bold());
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
    });

    // Check SSH readiness
    let instance_clone = instance.clone();
    let check_start = start_time.clone();
    let check_task = tokio::spawn(async move {
        loop {
            if check_start.elapsed() >= max_duration {
                return Err(NydusError::SshError("SSH did not become ready within 3 minutes".to_string()));
            }

            match crate::ssh::run_remote_command(&instance_clone, "echo 'ready'") {
                Ok(_) => return Ok(()),
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }
    });

    // Wait for SSH check to complete
    let result = check_task.await.map_err(|e| NydusError::SshError(format!("Task error: {}", e)))?;

    // Give the timer a moment to catch up and display the final time
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Calculate final elapsed time before stopping timer
    let final_elapsed = start_time.elapsed();

    // Stop the timer task
    timer_task.abort();

    // Clear line and show final message
    print!("\r\x1b[2K");

    match result {
        Ok(_) => {
            let final_time = format!("{:.1}s", final_elapsed.as_secs_f64());
            print!("✓ SSH is ready! ");
            println!("{}", format!("({})", final_time).dimmed());
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Resolve the latest Ubuntu LTS AMI
async fn resolve_latest_ubuntu_ami(client: &Client) -> Result<String> {
    // Search for Ubuntu 22.04 LTS (amd64)
    let result = client
        .describe_images()
        .owners("099720109477") // Canonical's AWS account ID
        .filters(
            Filter::builder()
                .name("name")
                .values("ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*")
                .build(),
        )
        .filters(
            Filter::builder()
                .name("state")
                .values("available")
                .build(),
        )
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to find Ubuntu AMI: {}", e)))?;

    // Get the most recent AMI
    let ami = result
        .images()
        .iter()
        .max_by_key(|img| img.creation_date().unwrap_or(""))
        .ok_or_else(|| NydusError::AwsError("No Ubuntu AMI found".to_string()))?;

    ami.image_id()
        .ok_or_else(|| NydusError::AwsError("AMI has no ID".to_string()))
        .map(|s| s.to_string())
}

/// Ensure a security group exists for nydus, create if needed
async fn ensure_security_group(client: &Client, instance_name: &str) -> Result<String> {
    let group_name = format!("nydus-{}", instance_name);
    let description = format!("Security group for nydus instance {}", instance_name);

    // Check if group already exists
    let existing = client
        .describe_security_groups()
        .group_names(&group_name)
        .send()
        .await;

    if let Ok(result) = existing {
        if let Some(group) = result.security_groups().first() {
            if let Some(group_id) = group.group_id() {
                return Ok(group_id.to_string());
            }
        }
    }

    // Get my public IP for SSH restriction
    let my_ip = get_my_public_ip().await?;

    // Create new security group
    let create_result = client
        .create_security_group()
        .group_name(&group_name)
        .description(&description)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to create security group: {}", e)))?;

    let group_id = create_result
        .group_id()
        .ok_or_else(|| NydusError::AwsError("No group ID returned".to_string()))?
        .to_string();

    // Add SSH rule
    client
        .authorize_security_group_ingress()
        .group_id(&group_id)
        .ip_protocol("tcp")
        .from_port(22)
        .to_port(22)
        .cidr_ip(format!("{}/32", my_ip))
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to add SSH rule: {}", e)))?;

    println!("Created security group: {}", group_name);

    Ok(group_id)
}

/// Get my public IP address
pub async fn get_my_public_ip() -> Result<String> {
    // Use ipify.org to get public IP
    let response = reqwest::get("https://api.ipify.org")
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to get public IP: {}", e)))?;

    let ip = response
        .text()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to read public IP: {}", e)))?;

    Ok(ip.trim().to_string())
}

/// Discover all nydus-managed instances from AWS across specified regions
pub async fn discover_nydus_instances(regions: Vec<String>) -> Result<Vec<Instance>> {
    let mut discovered_instances = Vec::new();

    for region in regions {
        match discover_nydus_instances_in_region(&region).await {
            Ok(mut instances) => {
                discovered_instances.append(&mut instances);
            }
            Err(e) => {
                eprintln!("⚠ Failed to query region {}: {}", region, e);
            }
        }
    }

    Ok(discovered_instances)
}

/// Discover nydus-managed instances in a specific region
async fn discover_nydus_instances_in_region(region: &str) -> Result<Vec<Instance>> {
    let client = initialize_ec2_client(region).await?;

    // Query for instances with ManagedBy: nydus tag
    let filter = Filter::builder()
        .name("tag:ManagedBy")
        .values("nydus")
        .build();

    let describe_result = client
        .describe_instances()
        .filters(filter)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe instances: {:?}", e)))?;

    let mut instances = Vec::new();

    for reservation in describe_result.reservations() {
        for ec2_instance in reservation.instances() {
            // Extract instance metadata from tags
            let tags: std::collections::HashMap<String, String> = ec2_instance
                .tags()
                .iter()
                .filter_map(|tag| {
                    let key = tag.key()?;
                    let value = tag.value()?;
                    Some((key.to_string(), value.to_string()))
                })
                .collect();

            let instance_id = ec2_instance.instance_id().unwrap_or("unknown").to_string();
            let name = tags.get("Name").cloned().unwrap_or(instance_id.clone());
            let profile = tags.get("nydus:profile").cloned().unwrap_or("default".to_string());
            let ssh_user = tags.get("nydus:ssh-user").cloned().unwrap_or("ubuntu".to_string());
            let ssh_key_path = tags
                .get("nydus:ssh-key-path")
                .cloned()
                .unwrap_or("~/.ssh/id_ed25519".to_string());
            let created_at = tags
                .get("nydus:created-at")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or_else(|| chrono::Utc::now().timestamp());

            let public_ip = ec2_instance
                .public_ip_address()
                .map(|s| s.to_string());
            let public_dns = ec2_instance
                .public_dns_name()
                .map(|s| s.to_string());
            let private_ip = ec2_instance
                .private_ip_address()
                .map(|s| s.to_string());

            let status = ec2_instance
                .state()
                .and_then(|s| s.name())
                .map(|s| format!("{:?}", s))
                .unwrap_or("unknown".to_string());

            let instance = Instance {
                id: None, // Will be set when saved to DB
                name,
                profile,
                region: region.to_string(),
                instance_id,
                public_ip,
                public_dns,
                private_ip,
                ssh_user,
                ssh_key_path,
                created_at,
                last_seen: chrono::Utc::now().timestamp(),
                last_synced: None,
                desired_state: "running".to_string(),
                status: Some(status),
                notes: None,
            };

            instances.push(instance);
        }
    }

    Ok(instances)
}

/// Check if an IP is allowed in the instance's security group
pub async fn check_ip_allowed(region: &str, instance_id: &str, ip: &str) -> Result<bool> {
    let client = initialize_ec2_client(region).await?;

    // Get the instance to find its security group
    let describe_instances = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe instance: {:?}", e)))?;

    let instance = describe_instances
        .reservations()
        .first()
        .and_then(|r| r.instances().first())
        .ok_or_else(|| NydusError::AwsError("Instance not found".to_string()))?;

    // Get the first security group from the instance
    let sg_id = instance
        .security_groups()
        .first()
        .and_then(|sg| sg.group_id())
        .ok_or_else(|| NydusError::AwsError("Instance has no security group".to_string()))?;

    // Get security group details
    let describe_result = client
        .describe_security_groups()
        .group_ids(sg_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe security group: {:?}", e)))?;

    let security_group = describe_result
        .security_groups()
        .first()
        .ok_or_else(|| NydusError::AwsError("Security group not found".to_string()))?;

    // Check if IP is allowed for SSH (port 22)
    let ip_with_cidr = format!("{}/32", ip);
    for permission in security_group.ip_permissions() {
        if let Some(port) = permission.from_port() {
            if port == 22 {
                // Check if our IP is in the allowed ranges
                for ip_range in permission.ip_ranges() {
                    if let Some(cidr) = ip_range.cidr_ip() {
                        if cidr == ip_with_cidr || cidr == "0.0.0.0/0" {
                            return Ok(true);
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

/// Update security group to allow SSH from a new IP address
pub async fn update_security_group_ip(region: &str, instance_id: &str, new_ip: &str) -> Result<()> {
    let client = initialize_ec2_client(region).await?;

    // Get the instance to find its security group
    let describe_instances = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe instance: {:?}", e)))?;

    let instance = describe_instances
        .reservations()
        .first()
        .and_then(|r| r.instances().first())
        .ok_or_else(|| NydusError::AwsError("Instance not found".to_string()))?;

    // Get the first security group from the instance
    let sg_id = instance
        .security_groups()
        .first()
        .and_then(|sg| sg.group_id())
        .ok_or_else(|| NydusError::AwsError("Instance has no security group".to_string()))?;

    // Get security group details
    let describe_result = client
        .describe_security_groups()
        .group_ids(sg_id)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to describe security group: {:?}", e)))?;

    let security_group = describe_result
        .security_groups()
        .first()
        .ok_or_else(|| NydusError::AwsError("Security group not found".to_string()))?;

    // Remove existing SSH rules (we'll replace them)
    for permission in security_group.ip_permissions() {
        // Only revoke SSH rules (port 22)
        if let Some(port) = permission.from_port() {
            if port == 22 {
                client
                    .revoke_security_group_ingress()
                    .group_id(sg_id)
                    .ip_permissions(permission.clone())
                    .send()
                    .await
                    .ok(); // Ignore errors if rule doesn't exist
            }
        }
    }

    // Add new SSH rule with current IP
    use aws_sdk_ec2::types::{IpPermission, IpRange};

    let ip_range = IpRange::builder()
        .cidr_ip(format!("{}/32", new_ip))
        .description("SSH access from current IP (updated by nydus)")
        .build();

    let permission = IpPermission::builder()
        .ip_protocol("tcp")
        .from_port(22)
        .to_port(22)
        .ip_ranges(ip_range)
        .build();

    client
        .authorize_security_group_ingress()
        .group_id(sg_id)
        .ip_permissions(permission)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to update security group: {:?}", e)))?;

    Ok(())
}
