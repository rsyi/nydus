use crate::config::{Instance, Profile};
use crate::error::{NydusError, Result};
use crate::util;
use aws_sdk_ec2::types::{Filter, InstanceStateName, ResourceType, Tag, TagSpecification};
use aws_sdk_ec2::Client;

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
        Tag::builder()
            .key("ManagedBy")
            .value("nydus")
            .build(),
    ];

    for (key, value) in &profile.tags {
        tags.push(Tag::builder().key(key).value(value).build());
    }

    let tag_spec = TagSpecification::builder()
        .resource_type(ResourceType::Instance)
        .set_tags(Some(tags))
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
        .tag_specifications(tag_spec)
        .send()
        .await
        .map_err(|e| NydusError::AwsError(format!("Failed to launch instance: {}", e)))?;

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
    describe_instance(&client, &instance_id, name, profile).await
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
    loop {
        let result = client
            .describe_instances()
            .instance_ids(instance_id)
            .send()
            .await
            .map_err(|e| NydusError::AwsError(format!("Failed to describe instance: {}", e)))?;

        let state = result
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .and_then(|i| i.state())
            .and_then(|s| s.name());

        match state {
            Some(InstanceStateName::Running) => {
                println!("Instance is running");
                return Ok(());
            }
            Some(InstanceStateName::Terminated) | Some(InstanceStateName::ShuttingDown) => {
                return Err(NydusError::AwsError("Instance terminated".to_string()));
            }
            _ => {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
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
async fn get_my_public_ip() -> Result<String> {
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
