use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "nydus")]
#[command(about = "Local-first EC2 instance manager for remote dev environments", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize nydus configuration directory
    Init,

    /// Launch interactive TUI (default if no command specified)
    Tui,

    /// Create and start a new instance
    Up {
        /// Name for the instance
        name: String,

        /// Profile to use
        #[arg(short, long)]
        profile: String,

        /// Skip credential sync
        #[arg(long)]
        no_sync: bool,
    },

    /// Stop a running instance
    Down {
        /// Instance name (uses current context if not specified)
        name: Option<String>,
    },

    /// Terminate an instance
    Terminate {
        /// Instance name
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// List all instances
    Ls,

    /// Import an existing EC2 instance
    Import {
        /// EC2 instance ID
        #[arg(long)]
        instance_id: String,

        /// Name for the instance
        #[arg(long)]
        name: String,

        /// AWS region
        #[arg(long)]
        region: String,

        /// SSH user
        #[arg(long)]
        ssh_user: String,

        /// Path to SSH private key
        #[arg(long)]
        key: String,

        /// Profile to associate with
        #[arg(short, long)]
        profile: String,
    },

    /// Attach to instance via SSH
    Attach {
        /// Instance name (uses current context if not specified)
        name: Option<String>,
    },

    /// Create SSH port forward
    Forward {
        /// Instance name (uses current context if not specified)
        name: Option<String>,

        /// Remote port
        #[arg(long)]
        remote: u16,

        /// Local port (auto-assigned if not specified)
        #[arg(long)]
        local: Option<u16>,

        /// Remote host (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        remote_host: String,

        /// Run in background
        #[arg(short, long)]
        background: bool,

        /// Open browser after establishing tunnel
        #[arg(short, long)]
        open: bool,
    },

    /// Open browser to forwarded port
    Open {
        /// Instance name (uses current context if not specified)
        name: Option<String>,

        /// Remote port
        #[arg(long)]
        remote: u16,
    },

    /// Set current context to an instance
    Switch {
        /// Instance name
        name: String,
    },

    /// Sync credentials to instance
    Sync {
        /// Instance name (uses current context if not specified)
        name: Option<String>,
    },

    /// List active tunnels
    Tunnels {
        /// Instance name (show all if not specified)
        name: Option<String>,
    },

    /// Stop a tunnel
    TunnelStop {
        /// Tunnel ID
        id: i64,
    },

    /// Profile management commands
    #[command(subcommand)]
    Profile(ProfileCommands),
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// List all profiles
    List,

    /// Show profile details
    Show {
        /// Profile name
        name: String,
    },

    /// Add a new profile
    Add {
        /// Profile name
        name: String,
    },
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Commands::Init) => {
            println!("Initializing nydus...");
            crate::config::init_nydus_dir()?;
            println!("✓ Created ~/.nydus/ directory");
            println!("✓ Created config.yaml");
            println!("✓ Created state.sqlite database");
            Ok(())
        }
        Some(Commands::Tui) | None => {
            // Launch TUI
            crate::tui::run().await
        }
        Some(Commands::Up { name, profile, no_sync }) => {
            cmd_up(&name, &profile, !no_sync).await
        }
        Some(Commands::Down { name }) => {
            cmd_down(name.as_deref()).await
        }
        Some(Commands::Terminate { name, yes }) => {
            cmd_terminate(&name, yes).await
        }
        Some(Commands::Ls) => {
            cmd_ls().await
        }
        Some(Commands::Import { instance_id, name, region, ssh_user, key, profile }) => {
            cmd_import(&instance_id, &name, &region, &ssh_user, &key, &profile).await
        }
        Some(Commands::Switch { name }) => {
            cmd_switch(&name).await
        }
        Some(Commands::Attach { name }) => {
            cmd_attach(name.as_deref()).await
        }
        Some(Commands::Forward { name, remote, local, remote_host, background, open }) => {
            cmd_forward(name.as_deref(), remote, local, &remote_host, background, open).await
        }
        Some(Commands::Open { name, remote }) => {
            cmd_open(name.as_deref(), remote).await
        }
        Some(Commands::Sync { name }) => {
            cmd_sync(name.as_deref()).await
        }
        Some(Commands::Tunnels { name }) => {
            cmd_tunnels(name.as_deref()).await
        }
        Some(Commands::TunnelStop { id }) => {
            cmd_tunnel_stop(id).await
        }
        Some(Commands::Profile(profile_cmd)) => {
            cmd_profile(profile_cmd).await
        }
        _ => {
            println!("Command not yet implemented");
            Ok(())
        }
    }
}

async fn cmd_up(name: &str, profile_name: &str, sync: bool) -> Result<()> {
    let config = crate::config::Config::load()?;
    let profile = config.get_profile(profile_name)?;

    println!("Creating instance '{}' with profile '{}'...", name, profile_name);

    // Create EC2 instance
    let instance = crate::aws::ec2::run_instance(profile, name).await?;

    println!("✓ Instance created: {}", instance.instance_id);
    if let Some(ip) = &instance.public_ip {
        println!("✓ Public IP: {}", ip);
    }

    // Save to state database
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    db.upsert_instance(&instance)?;
    db.set_current_context(Some(name))?;

    println!("✓ Saved to local state");

    // Sync credentials if enabled
    if sync && profile.sync_credentials.enabled {
        crate::sync::sync_credentials(&instance, profile)?;

        // Update last_synced timestamp
        let mut updated = instance.clone();
        updated.last_synced = Some(crate::util::current_timestamp());
        db.upsert_instance(&updated)?;
    }

    println!("\n✓ Instance ready: {} ({})", name, instance.public_ip.unwrap_or_default());
    println!("  Connect with: nydus attach {}", name);

    Ok(())
}

async fn cmd_down(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;

    println!("Stopping instance '{}'...", instance_name);
    crate::aws::ec2::stop_instance(&instance).await?;

    // Update state
    let mut updated = instance.clone();
    updated.desired_state = "stopped".to_string();
    updated.status = Some("stopping".to_string());
    db.upsert_instance(&updated)?;

    println!("✓ Instance stopped");

    Ok(())
}

async fn cmd_terminate(name: &str, yes: bool) -> Result<()> {
    if !yes {
        println!("This will permanently delete the instance '{}'.", name);
        println!("Are you sure? (y/N): ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    let instance = db.get_instance(name)?;

    println!("Terminating instance '{}'...", name);
    crate::aws::ec2::terminate_instance(&instance).await?;

    // Remove from state
    db.delete_instance(name)?;

    // Clear context if this was the current instance
    if let Some(current) = db.get_current_context()? {
        if current == name {
            db.set_current_context(None)?;
        }
    }

    println!("✓ Instance terminated and removed from state");

    Ok(())
}

async fn cmd_ls() -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    let instances = db.list_instances()?;

    if instances.is_empty() {
        println!("No instances found.");
        println!("Create one with: nydus up <name> --profile <profile>");
        return Ok(());
    }

    let current_context = db.get_current_context()?;

    println!("\n{:<20} {:<15} {:<20} {:<15} {:<12}", "NAME", "STATUS", "INSTANCE ID", "PUBLIC IP", "REGION");
    println!("{}", "-".repeat(85));

    for instance in instances {
        let is_current = current_context.as_ref().map_or(false, |c| c == &instance.name);
        let marker = if is_current { "*" } else { " " };

        println!(
            "{}{:<19} {:<15} {:<20} {:<15} {:<12}",
            marker,
            instance.name,
            instance.status.as_deref().unwrap_or("unknown"),
            instance.instance_id,
            instance.public_ip.as_deref().unwrap_or("-"),
            instance.region
        );
    }

    if let Some(current) = current_context {
        println!("\n* Current context: {}", current);
    }

    Ok(())
}

async fn cmd_import(
    instance_id: &str,
    name: &str,
    region: &str,
    ssh_user: &str,
    key_path: &str,
    profile_name: &str,
) -> Result<()> {
    let config = crate::config::Config::load()?;
    let profile = config.get_profile(profile_name)?;

    println!("Importing instance {}...", instance_id);

    let client = crate::aws::ec2::initialize_ec2_client(region).await?;

    // Fetch instance details
    let mut instance = crate::aws::ec2::describe_instance(&client, instance_id, name, profile).await?;

    // Override with provided values
    instance.ssh_user = ssh_user.to_string();
    instance.ssh_key_path = crate::util::expand_tilde(key_path)?
        .to_string_lossy()
        .to_string();

    // Save to state
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    db.upsert_instance(&instance)?;

    println!("✓ Imported instance: {}", name);
    println!("  Instance ID: {}", instance_id);
    println!("  Public IP: {}", instance.public_ip.as_deref().unwrap_or("N/A"));

    Ok(())
}

async fn cmd_switch(name: &str) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    // Verify instance exists
    db.get_instance(name)?;

    db.set_current_context(Some(name))?;

    println!("✓ Switched to instance: {}", name);

    Ok(())
}

async fn cmd_attach(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;

    crate::ssh::attach(&instance)?;

    Ok(())
}

async fn cmd_forward(
    name: Option<&str>,
    remote_port: u16,
    local_port: Option<u16>,
    remote_host: &str,
    background: bool,
    open_browser: bool,
) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;

    // Find available local port if not specified
    let local_port = if let Some(port) = local_port {
        port
    } else {
        crate::util::find_available_port(remote_port)?
    };

    let tunnel = crate::config::Tunnel {
        id: None,
        instance_name: instance_name.clone(),
        remote_host: remote_host.to_string(),
        remote_port,
        local_port,
        pid: None,
        started_at: crate::util::current_timestamp(),
        mode: if background {
            "background".to_string()
        } else {
            "foreground".to_string()
        },
        open_url: open_browser,
    };

    if background {
        let pid = crate::ssh::forward_background(&instance, &tunnel)?;

        // Save tunnel to database
        let mut tunnel_with_pid = tunnel.clone();
        tunnel_with_pid.pid = Some(pid);
        db.create_tunnel(&tunnel_with_pid)?;

        println!("✓ Tunnel started (PID: {})", pid);
        println!("  Local: localhost:{}", local_port);
        println!("  Remote: {}:{}", remote_host, remote_port);

        if open_browser {
            let url = format!("http://localhost:{}", local_port);
            crate::util::open_browser(&url)?;
            println!("  Opened browser: {}", url);
        }
    } else {
        // Foreground mode - blocking
        crate::ssh::forward_foreground(&instance, &tunnel)?;
    }

    Ok(())
}

async fn cmd_open(name: Option<&str>, remote_port: u16) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    // Find active tunnel for this remote port
    let tunnels = db.get_active_tunnels(&instance_name)?;
    let tunnel = tunnels
        .iter()
        .find(|t| t.remote_port == remote_port)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No active tunnel found for remote port {}. Create one with: nydus forward --remote {}",
                remote_port,
                remote_port
            )
        })?;

    let url = format!("http://localhost:{}", tunnel.local_port);
    println!("Opening browser: {}", url);
    crate::util::open_browser(&url)?;

    Ok(())
}

async fn cmd_sync(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;
    let config = crate::config::Config::load()?;
    let profile = config.get_profile(&instance.profile)?;

    crate::sync::sync_credentials(&instance, profile)?;

    // Update last_synced timestamp
    let mut updated = instance.clone();
    updated.last_synced = Some(crate::util::current_timestamp());
    db.upsert_instance(&updated)?;

    Ok(())
}

async fn cmd_tunnels(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let tunnels = db.list_tunnels(name)?;

    if tunnels.is_empty() {
        println!("No tunnels found.");
        return Ok(());
    }

    println!(
        "\n{:<5} {:<20} {:<15} {:<12} {:<12} {:<10}",
        "ID", "INSTANCE", "REMOTE", "LOCAL", "PID", "STATUS"
    );
    println!("{}", "-".repeat(80));

    for tunnel in tunnels {
        let is_alive = tunnel.pid.map_or(false, |p| crate::util::is_process_alive(p));
        let status = if is_alive { "active" } else { "stopped" };
        let pid_str = tunnel.pid.map_or("-".to_string(), |p| p.to_string());

        println!(
            "{:<5} {:<20} {:<15} {:<12} {:<12} {:<10}",
            tunnel.id.unwrap_or(0),
            tunnel.instance_name,
            format!("{}:{}", tunnel.remote_host, tunnel.remote_port),
            tunnel.local_port,
            pid_str,
            status
        );
    }

    Ok(())
}

async fn cmd_tunnel_stop(id: i64) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let tunnels = db.list_tunnels(None)?;
    let tunnel = tunnels
        .iter()
        .find(|t| t.id == Some(id))
        .ok_or_else(|| anyhow::anyhow!("Tunnel not found: {}", id))?;

    if let Some(pid) = tunnel.pid {
        if crate::util::is_process_alive(pid) {
            crate::ssh::stop_tunnel(pid)?;
            println!("✓ Stopped tunnel {} (PID: {})", id, pid);
        } else {
            println!("⚠ Tunnel {} already stopped", id);
        }
    }

    // Remove from database
    db.delete_tunnel(id)?;

    Ok(())
}

async fn cmd_profile(cmd: ProfileCommands) -> Result<()> {
    match cmd {
        ProfileCommands::List => {
            let config = crate::config::Config::load()?;
            println!("\nProfiles:");
            for profile in &config.profiles {
                println!("  - {} ({})", profile.name, profile.region);
            }
        }
        ProfileCommands::Show { name } => {
            let config = crate::config::Config::load()?;
            let profile = config.get_profile(&name)?;
            println!("\nProfile: {}", profile.name);
            println!("  Region: {}", profile.region);
            println!("  Instance Type: {}", profile.instance_type);
            println!("  SSH User: {}", profile.ssh_user);
            println!("  SSH Key: {}", profile.ssh_key_path);
            println!("  Volume Size: {}GB", profile.volume_size_gb);
            if let Some(ami) = &profile.ami {
                println!("  AMI: {}", ami);
            }
            if !profile.tags.is_empty() {
                println!("  Tags:");
                for (k, v) in &profile.tags {
                    println!("    {}: {}", k, v);
                }
            }
            println!("  Credential Sync: {}", profile.sync_credentials.enabled);
        }
        ProfileCommands::Add { name } => {
            let mut config = crate::config::Config::load()?;
            let new_profile = crate::config::Profile {
                name: name.clone(),
                ..Default::default()
            };
            config.upsert_profile(new_profile);
            config.save()?;
            println!("✓ Added profile: {}", name);
            println!("  Edit ~/.nydus/config.yaml to configure");
        }
    }
    Ok(())
}
