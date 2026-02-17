use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::*;

#[derive(Parser)]
#[command(name = "nydus")]
#[command(version)]
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

        /// Profile to use (defaults to "default")
        #[arg(short, long, default_value = "default")]
        profile: String,

        /// Skip credential sync
        #[arg(long)]
        no_sync: bool,
    },

    /// Terminate and destroy an instance
    Down {
        /// Instance name (uses current context if not specified)
        name: Option<String>,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Stop a running instance (can be restarted)
    Stop {
        /// Instance name (uses current context if not specified)
        name: Option<String>,
    },

    /// Start a stopped instance
    Start {
        /// Instance name (uses current context if not specified)
        name: Option<String>,
    },

    /// List all instances
    Ls {
        /// Skip refreshing status from AWS (faster but may be stale)
        #[arg(long)]
        no_refresh: bool,
    },

    /// Show detailed instance status with tmux sessions and running processes
    Status {
        /// Instance name (uses current context if not specified)
        name: Option<String>,
    },

    /// Refresh instance status from AWS
    Refresh {
        /// Instance name (refresh all if not specified)
        name: Option<String>,
    },

    /// Remove instance from local state (doesn't touch AWS)
    Forget {
        /// Instance name
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

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
    ///
    /// PORT format: REMOTE or REMOTE:LOCAL (e.g. 8080 or 8080:9000)
    Forward {
        /// Instance name (uses current context if not specified)
        name: Option<String>,

        /// Port mapping: REMOTE or REMOTE:LOCAL (e.g. 8080 or 8080:9000)
        #[arg(value_name = "PORT")]
        port: String,

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

    /// Sync local state from AWS (discover nydus-managed instances)
    SyncState {
        /// AWS regions to query (defaults to profile regions)
        #[arg(short, long)]
        regions: Option<Vec<String>>,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Update security group to allow SSH from current IP
    UpdateIp {
        /// Instance name (updates all instances if not specified)
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
            println!("{}", "Initializing nydus...".cyan());
            let result = crate::config::init_nydus_dir()?;
            if result.dir_created {
                println!("{}", "✓ Created ~/.nydus/ directory".green());
            } else {
                println!("{}", "~ ~/.nydus/ already exists".dimmed());
            }
            if result.config_created {
                println!("{}", "✓ Created config.yaml".green());
            } else {
                println!("{}", "~ config.yaml already exists (not overwritten)".dimmed());
            }
            if result.db_created {
                println!("{}", "✓ Created state.sqlite database".green());
            } else {
                println!("{}", "~ state.sqlite already exists (not overwritten)".dimmed());
            }
            Ok(())
        }
        Some(Commands::Tui) | None => {
            // Launch TUI
            crate::tui::run().await
        }
        Some(Commands::Up { name, profile, no_sync }) => {
            cmd_up(&name, &profile, !no_sync).await
        }
        Some(Commands::Down { name, yes }) => {
            cmd_down(name.as_deref(), yes).await
        }
        Some(Commands::Stop { name }) => {
            cmd_stop(name.as_deref()).await
        }
        Some(Commands::Start { name }) => {
            cmd_start(name.as_deref()).await
        }
        Some(Commands::Ls { no_refresh }) => {
            cmd_ls(!no_refresh).await
        }
        Some(Commands::Status { name }) => {
            cmd_status(name.as_deref()).await
        }
        Some(Commands::Refresh { name }) => {
            cmd_refresh(name.as_deref()).await
        }
        Some(Commands::Forget { name, yes }) => {
            cmd_forget(&name, yes).await
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
        Some(Commands::Forward { name, port, remote_host, background, open }) => {
            let (remote, local) = parse_port_mapping(&port)?;
            cmd_forward(name.as_deref(), remote, local, &remote_host, background, open).await
        }
        Some(Commands::Open { name, remote }) => {
            cmd_open(name.as_deref(), remote).await
        }
        Some(Commands::Sync { name }) => {
            cmd_sync(name.as_deref()).await
        }
        Some(Commands::SyncState { regions, yes }) => {
            cmd_sync_state(regions.as_deref(), yes).await
        }
        Some(Commands::UpdateIp { name }) => {
            cmd_update_ip(name.as_deref()).await
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

    // Check if instance name already exists in local state
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    if let Ok(existing) = db.get_instance(name) {
        let status = existing.status.as_deref().unwrap_or("unknown").to_lowercase();

        eprintln!("{}", format!("✗ Instance '{}' already exists.", name).red().bold());
        eprintln!();

        if status.contains("stop") {
            eprintln!("{}", "Instance is stopped. To restart it:".yellow());
            eprintln!("  {} {}", "→".green(), format!("nydus start {}", name).bright_white().bold());
        } else if status.contains("running") {
            eprintln!("{}", "Instance is running.".yellow());
            eprintln!("  {} {}", "→".green(), format!("nydus attach {}", name).bright_white());
        } else {
            eprintln!("{}", "Available options:".yellow());
            eprintln!("  {} {}", "•".yellow(), format!("View:      nydus ls").bright_white());
            eprintln!("  {} {}", "•".yellow(), format!("Refresh:   nydus refresh {}", name).bright_white());
        }

        eprintln!();
        eprintln!("  {} {}", "Remove:".dimmed(), format!("nydus terminate {} --yes", name).dimmed());
        eprintln!();
        eprintln!("{}", format!("Status: {} ({})",
            existing.status.as_deref().unwrap_or("unknown"),
            existing.instance_id
        ).dimmed());

        return Err(anyhow::anyhow!("Instance already exists"));
    }

    println!("{}", format!("Creating instance '{}' with profile '{}'...", name, profile_name).cyan());

    // Create EC2 instance
    let instance = crate::aws::ec2::run_instance(profile, name).await?;

    println!("{} {}", "✓ Instance created:".green().bold(), instance.instance_id.bright_white());
    if let Some(ip) = &instance.public_ip {
        println!("{} {}", "✓ Public IP:".green().bold(), ip.bright_white());
    }

    // Save to state database
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    db.upsert_instance(&instance)?;
    db.set_current_context(Some(name))?;

    println!("{}", "✓ Saved to local state".green().bold());

    // Sync credentials if enabled
    if sync && profile.sync_credentials.enabled {
        println!();
        crate::sync::sync_credentials(&instance, profile)?;

        // Update last_synced timestamp
        let mut updated = instance.clone();
        updated.last_synced = Some(crate::util::current_timestamp());
        db.upsert_instance(&updated)?;
    }

    println!();
    println!("{} {} {}",
        "🚀".normal(),
        name.bright_white().bold(),
        "is ready!".green().bold()
    );
    println!("   {} {}", "IP:".dimmed(), instance.public_ip.unwrap_or_default().bright_cyan());
    println!();
    println!("   {} {}", "Connect:".bright_white(), format!("nydus attach {}", name).truecolor(255, 165, 0).bold());

    Ok(())
}

async fn cmd_down(name: Option<&str>, yes: bool) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    if !yes {
        println!("{}", format!("This will permanently terminate instance '{}'.", instance_name).yellow());
        print!("Are you sure? (y/N): ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let instance = db.get_instance(&instance_name)?;

    println!("{}", format!("Terminating instance '{}'...", instance_name).cyan());
    crate::aws::ec2::terminate_instance(&instance).await?;

    // Remove from state
    db.delete_instance(&instance_name)?;

    // Clear context if this was the current instance
    if let Some(current) = db.get_current_context()? {
        if current == instance_name {
            db.set_current_context(None)?;
        }
    }

    println!("{}", "✓ Instance terminated and removed from state".green());

    Ok(())
}

async fn cmd_stop(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;

    println!("{}", format!("Stopping instance '{}'...", instance_name).cyan());
    crate::aws::ec2::stop_instance(&instance).await?;

    // Update state
    let mut updated = instance.clone();
    updated.desired_state = "stopped".to_string();
    updated.status = Some("stopping".to_string());
    db.upsert_instance(&updated)?;

    println!("{}", "✓ Instance stopped (use 'nydus start' to restart)".green());

    Ok(())
}

async fn cmd_start(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;

    println!("{}", format!("Starting instance '{}'...", instance_name).cyan());
    crate::aws::ec2::start_instance(&instance).await?;

    // Refresh and update state
    let updated = crate::aws::ec2::refresh_instance(&instance).await?;
    db.upsert_instance(&updated)?;
    db.set_current_context(Some(&instance_name))?;

    println!("{} {}", "✓ Instance started:".green(), updated.public_ip.unwrap_or_default().bright_white());
    println!("  {} {}", "Connect with:".yellow(), format!("nydus attach {}", instance_name).bright_white());

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

async fn cmd_ls(refresh: bool) -> Result<()> {
    if refresh {
        println!("{}", "Refreshing instance status from AWS...".cyan());
        cmd_refresh(None).await?;
        println!();
    }

    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    let instances = db.list_instances()?;

    if instances.is_empty() {
        println!("{}", "No instances found.".yellow());
        println!("{} {}", "Create one with:".dimmed(), "nydus up <name>".bright_white());
        return Ok(());
    }

    let current_context = db.get_current_context()?;

    println!();
    println!("{:<20} {:<15} {:<20} {:<15} {:<12}",
        "NAME".bright_white().bold(),
        "STATUS".bright_white().bold(),
        "INSTANCE ID".bright_white().bold(),
        "PUBLIC IP".bright_white().bold(),
        "REGION".bright_white().bold()
    );
    println!("{}", "-".repeat(85).dimmed());

    for instance in instances {
        let is_current = current_context.as_ref().map_or(false, |c| c == &instance.name);
        let marker = if is_current { "→".green() } else { " ".normal() };

        let status = instance.status.as_deref().unwrap_or("unknown");
        let status_colored = match status {
            s if s.contains("running") => s.green(),
            s if s.contains("stopped") => s.yellow(),
            s if s.contains("terminated") => s.red(),
            s if s.contains("stopping") || s.contains("pending") => s.cyan(),
            s => s.normal(),
        };

        println!(
            "{} {:<19} {:<15} {:<20} {:<15} {:<12}",
            marker,
            if is_current { instance.name.bright_cyan().bold().to_string() } else { instance.name.to_string() },
            status_colored,
            instance.instance_id.dimmed(),
            instance.public_ip.as_deref().unwrap_or("-").bright_white(),
            instance.region.dimmed()
        );
    }

    if let Some(current) = current_context {
        println!();
        println!("{} {}",
            "→ Current context:".green(),
            current.bright_cyan().bold()
        );
    }

    Ok(())
}

async fn cmd_status(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instance_name = if let Some(name) = name {
        name.to_string()
    } else {
        db.get_current_context()?
            .ok_or_else(|| anyhow::anyhow!("No current context. Specify instance name."))?
    };

    let instance = db.get_instance(&instance_name)?;

    // Refresh instance status
    let instance = crate::aws::ec2::refresh_instance(&instance).await?;
    let status = instance.status.as_deref().unwrap_or("unknown");

    // Print header
    println!();
    println!("{} {}", "Instance:".bright_white().bold(), instance.name.bright_cyan().bold());

    let status_lower = status.to_lowercase();
    let is_running = status_lower.contains("running");

    println!("{} {} ({})",
        "Status:  ".bright_white(),
        if is_running { "● running".green() } else { "○ stopped".yellow() },
        instance.public_ip.as_deref().unwrap_or("no IP").bright_white()
    );

    if !is_running {
        println!();
        println!("{}", "Instance is not running. Start it with:".yellow());
        println!("  {}", format!("nydus start {}", instance.name).bright_white().bold());
        println!();
        return Ok(());
    }

    // Get tmux state
    let tmux_output = match crate::ssh::run_remote_command(&instance,
        "tmux list-panes -a -F '#{session_name}|#{session_attached}|#{window_index}|#{window_name}|#{window_active}|#{pane_index}|#{pane_current_command}|#{pane_current_path}' 2>/dev/null || echo 'NO_TMUX'"
    ) {
        Ok(output) => output,
        Err(_) => {
            println!();
            println!("{}", "Could not query tmux state".yellow());
            return Ok(());
        }
    };

    if tmux_output.trim() == "NO_TMUX" {
        println!();
        println!("{}", "No tmux sessions found".dimmed());
    } else {
        display_tmux_tree(&tmux_output)?;
    }

    // Get running processes
    let claude_check = crate::ssh::run_remote_command(&instance,
        "pgrep -f claude > /dev/null && echo 'RUNNING' || echo 'NOT_RUNNING'"
    ).unwrap_or_default();

    println!();
    println!("{}", "Processes:".bright_white().bold());
    if claude_check.trim() == "RUNNING" {
        println!("  {} Claude Code", "●".green());
    } else {
        println!("  {} Claude Code", "○".dimmed());
    }

    // Get system info
    let uptime = crate::ssh::run_remote_command(&instance, "uptime -p").unwrap_or_default();
    println!();
    println!("{} {}", "Uptime:".bright_white(), uptime.trim().dimmed());

    println!();

    Ok(())
}

fn display_tmux_tree(output: &str) -> Result<()> {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct TmuxSession {
        attached: bool,
        windows: BTreeMap<usize, TmuxWindow>,
    }

    #[derive(Default)]
    struct TmuxWindow {
        name: String,
        active: bool,
        panes: BTreeMap<usize, TmuxPane>,
    }

    struct TmuxPane {
        command: String,
        path: String,
    }

    let mut sessions: BTreeMap<String, TmuxSession> = BTreeMap::new();

    // Parse tmux output
    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 8 {
            continue;
        }

        let session_name = parts[0].to_string();
        let session_attached = parts[1] == "1";
        let window_index: usize = parts[2].parse().unwrap_or(0);
        let window_name = parts[3].to_string();
        let window_active = parts[4] == "1";
        let pane_index: usize = parts[5].parse().unwrap_or(0);
        let pane_command = parts[6].to_string();
        let pane_path = parts[7].to_string();

        let session = sessions.entry(session_name).or_default();
        session.attached = session_attached;

        let window = session.windows.entry(window_index).or_default();
        window.name = window_name;
        window.active = window_active;

        window.panes.insert(pane_index, TmuxPane {
            command: pane_command,
            path: pane_path,
        });
    }

    // Display tree
    println!();
    println!("{} ({})", "📺 Tmux Sessions".bright_white().bold(), sessions.len());
    println!();

    for (session_name, session) in sessions.iter() {
        let attached_str = if session.attached { "[attached]".green() } else { "[detached]".dimmed() };
        println!("  {} {}", format!("Session: {}", session_name).bright_cyan(), attached_str);

        let window_count = session.windows.len();
        for (win_idx, (window_index, window)) in session.windows.iter().enumerate() {
            let is_last_window = win_idx == window_count - 1;
            let window_prefix = if is_last_window { "└─" } else { "├─" };
            let pane_prefix = if is_last_window { "   " } else { "│  " };

            let active_marker = if window.active { " ●".green() } else { "".normal() };
            println!("    {} Window {}: {}{}",
                window_prefix.dimmed(),
                window_index.to_string().bright_white(),
                window.name.bright_cyan().bold(),
                active_marker
            );

            let pane_count = window.panes.len();
            for (pane_idx, (pane_index, pane)) in window.panes.iter().enumerate() {
                let is_last_pane = pane_idx == pane_count - 1;
                let pane_branch = if is_last_pane { "└─" } else { "├─" };

                // Shorten path
                let short_path = pane.path.replace(&std::env::var("HOME").unwrap_or_default(), "~");

                println!("    {}  {} Pane {}: {:<15} {}",
                    pane_prefix.dimmed(),
                    pane_branch.dimmed(),
                    pane_index,
                    pane.command.dimmed(),
                    short_path.dimmed()
                );
            }
        }
        println!();
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

    // Proactively check if current IP is allowed
    let my_ip = crate::aws::ec2::get_my_public_ip().await?;
    let ip_allowed = crate::aws::ec2::check_ip_allowed(&instance.region, &instance.instance_id, &my_ip).await?;

    if !ip_allowed {
        println!();
        println!("{}", "Your current IP is not allowed in the security group!".yellow());
        println!("Current IP: {}", my_ip.bright_cyan());
        println!();

        use std::io::{self, Write};
        print!("{} ", "Update security group to allow your current IP? [Y/n]:".bright_white());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().is_empty() || input.trim().eq_ignore_ascii_case("y") {
            println!();
            println!("{} Updating security group...", "→".bright_white());
            crate::aws::ec2::update_security_group_ip(&instance.region, &instance.instance_id, &my_ip).await?;
            println!("  {} SSH access allowed from {}", "✓".green(), my_ip.bright_white());
            println!();
        } else {
            return Err(anyhow::anyhow!("Cannot connect - IP not allowed"));
        }
    }

    // Now attach
    crate::ssh::attach(&instance)?;
    Ok(())
}

fn parse_port_mapping(port: &str) -> Result<(u16, Option<u16>)> {
    if let Some((remote, local)) = port.split_once(':') {
        let remote = remote.parse::<u16>()
            .map_err(|_| anyhow::anyhow!("Invalid remote port: {}", remote))?;
        let local = local.parse::<u16>()
            .map_err(|_| anyhow::anyhow!("Invalid local port: {}", local))?;
        Ok((remote, Some(local)))
    } else {
        let remote = port.parse::<u16>()
            .map_err(|_| anyhow::anyhow!("Invalid port: {}. Use PORT or REMOTE:LOCAL (e.g. 8080 or 8080:9000)", port))?;
        Ok((remote, None))
    }
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

async fn cmd_sync_state(regions: Option<&[String]>, skip_confirm: bool) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;
    let config = crate::config::Config::load()?;

    // Determine regions to query
    let regions_to_query = if let Some(regions) = regions {
        regions.to_vec()
    } else {
        // Use regions from all profiles
        let mut profile_regions: std::collections::HashSet<String> = config
            .profiles
            .iter()
            .map(|p| p.region.clone())
            .collect();

        // Add current instances' regions
        for instance in db.list_instances()? {
            profile_regions.insert(instance.region.clone());
        }

        profile_regions.into_iter().collect()
    };

    println!("{}", "Discovering nydus-managed instances from AWS...".bright_white());
    println!("Regions: {}", regions_to_query.join(", ").dimmed());
    println!();

    // Discover instances from AWS
    let discovered = crate::aws::ec2::discover_nydus_instances(regions_to_query).await?;

    if discovered.is_empty() {
        println!("{}", "No nydus-managed instances found in AWS.".yellow());
        return Ok(());
    }

    // Show what will be synced
    println!("{} instances found:", discovered.len().to_string().bright_white());
    for instance in &discovered {
        let status_display = instance.status.as_deref().unwrap_or("unknown");
        let status_colored = if status_display.to_lowercase().contains("running") {
            status_display.green()
        } else {
            status_display.yellow()
        };

        println!(
            "  {} {} ({}) - {} in {}",
            "•".bright_white(),
            instance.name.bright_cyan(),
            status_colored,
            instance.instance_id.dimmed(),
            instance.region.dimmed()
        );
    }
    println!();

    // Prompt for confirmation unless --yes flag
    if !skip_confirm {
        use std::io::{self, Write};
        print!("{} ", "Sync these instances to local state? [y/N]:".bright_white());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Cancelled.".yellow());
            return Ok(());
        }
    }

    // Sync to local state
    println!("{}", "Syncing to local state...".bright_white());
    let mut synced_count = 0;
    let mut updated_count = 0;
    let mut skipped_count = 0;

    // Group by name to detect duplicates
    let mut name_to_instances: std::collections::HashMap<String, Vec<crate::config::Instance>> = std::collections::HashMap::new();
    for instance in &discovered {
        name_to_instances.entry(instance.name.clone()).or_default().push(instance.clone());
    }

    for mut instance in discovered {
        // Handle duplicate names
        let instances_with_name = &name_to_instances[&instance.name];
        if instances_with_name.len() > 1 {
            // Multiple instances with same name
            // If this is a terminated instance and there's a non-terminated one, skip it
            let is_terminated = instance.status.as_ref().map(|s| s.to_lowercase().contains("terminated")).unwrap_or(false);
            let has_active_duplicate = instances_with_name.iter().any(|i| {
                i.instance_id != instance.instance_id &&
                !i.status.as_ref().map(|s| s.to_lowercase().contains("terminated")).unwrap_or(true)
            });

            if is_terminated && has_active_duplicate {
                println!("  {} Skipped {} (terminated, active instance exists)", "⊘".yellow(), instance.name.dimmed());
                skipped_count += 1;
                continue;
            }

            // Otherwise append instance ID suffix to make unique
            let short_id = &instance.instance_id[instance.instance_id.len().saturating_sub(8)..];
            instance.name = format!("{}-{}", instance.name, short_id);
            println!("  {} Renamed to {} (duplicate name)", "✓".green(), instance.name.bright_white());
        }

        // Check if instance already exists in local state
        let existing = db.get_instance(&instance.name).ok();

        if existing.is_some() {
            updated_count += 1;
            println!("  {} Updated {}", "↻".yellow(), instance.name.dimmed());
        } else {
            synced_count += 1;
            println!("  {} Added {}", "✓".green(), instance.name.bright_white());
        }

        db.upsert_instance(&instance)?;
    }

    println!();
    if skipped_count > 0 {
        println!(
            "{} {} instances synced ({} new, {} updated, {} skipped)",
            "✓".green(),
            (synced_count + updated_count).to_string().bright_white(),
            synced_count.to_string().green(),
            updated_count.to_string().yellow(),
            skipped_count.to_string().dimmed()
        );
    } else {
        println!(
            "{} {} instances synced ({} new, {} updated)",
            "✓".green(),
            (synced_count + updated_count).to_string().bright_white(),
            synced_count.to_string().green(),
            updated_count.to_string().yellow()
        );
    }

    Ok(())
}

async fn cmd_update_ip(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    // Get current public IP
    println!("{}", "Getting your current public IP...".bright_white());
    let my_ip = crate::aws::ec2::get_my_public_ip().await?;
    println!("Current IP: {}", my_ip.bright_cyan());
    println!();

    // Get instances to update
    let instances: Vec<_> = if let Some(name) = name {
        vec![db.get_instance(name)?]
    } else {
        db.list_instances()?
    };

    if instances.is_empty() {
        println!("{}", "No instances found.".yellow());
        return Ok(());
    }

    // Update security group for each instance
    for instance in instances {
        println!(
            "{} Updating security group for {}...",
            "→".bright_white(),
            instance.name.bright_cyan()
        );

        // Update the security group
        crate::aws::ec2::update_security_group_ip(&instance.region, &instance.instance_id, &my_ip).await?;

        println!(
            "  {} SSH access allowed from {}",
            "✓".green(),
            my_ip.bright_white()
        );
    }

    println!();
    println!(
        "{} Security groups updated! You can now SSH from your current network.",
        "✓".green()
    );

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
            println!("{} {}", "✓ Added profile:".green(), name.bright_white());
            println!("  Edit ~/.nydus/config.yml to configure");
        }
    }
    Ok(())
}

async fn cmd_refresh(name: Option<&str>) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    let instances = if let Some(name) = name {
        vec![db.get_instance(name)?]
    } else {
        db.list_instances()?
    };

    if instances.is_empty() {
        println!("{}", "No instances to refresh.".yellow());
        return Ok(());
    }

    for instance in instances {
        print!("{} {}... ", "Refreshing".cyan(), instance.name.bright_white());
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match crate::aws::ec2::refresh_instance(&instance).await {
            Ok(updated) => {
                let status = updated.status.as_deref().unwrap_or("unknown");

                // Auto-remove terminated instances
                if status.contains("terminated") {
                    db.delete_instance(&instance.name)?;

                    // Clear context if this was the current instance
                    if let Some(current) = db.get_current_context()? {
                        if current == instance.name {
                            db.set_current_context(None)?;
                        }
                    }

                    println!("{}", "terminated (removed)".red().dimmed());
                } else {
                    db.upsert_instance(&updated)?;
                    let status_colored = match status {
                        s if s.contains("running") => s.green(),
                        s if s.contains("stopped") => s.yellow(),
                        s => s.normal(),
                    };
                    println!("{}", status_colored);
                }
            }
            Err(e) => {
                // Check if it's an InstanceNotFound error - instance was terminated and removed from AWS
                let err_msg = format!("{}", e);
                if err_msg.contains("InstanceNotFound") || err_msg.contains("Instance not found") {
                    // Instance not found in AWS - remove from local state
                    db.delete_instance(&instance.name)?;

                    // Clear context if this was the current instance
                    if let Some(current) = db.get_current_context()? {
                        if current == instance.name {
                            db.set_current_context(None)?;
                        }
                    }

                    println!("{}", "not found in AWS (removed)".red().dimmed());
                } else {
                    println!("{}", format!("error: {}", e).red());
                    println!("{} {}", "  Consider running:".yellow(), format!("nydus forget {}", instance.name).bright_white());
                }
            }
        }
    }

    Ok(())
}

async fn cmd_forget(name: &str, yes: bool) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    let db = crate::state::StateDb::open(&db_path)?;

    // Check if instance exists
    let instance = db.get_instance(name)?;

    if !yes {
        println!("{}", format!("Remove '{}' from local state?", name).yellow());
        println!("{}", "  This will NOT terminate the EC2 instance if it still exists.".dimmed());
        println!("  {}{}", "Instance ID: ".dimmed(), instance.instance_id);
        println!();
        print!("{}", "Continue? (y/N): ".bright_white());
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Aborted.".yellow());
            return Ok(());
        }
    }

    // Remove from state
    db.delete_instance(name)?;

    // Clear context if this was the current instance
    if let Some(current) = db.get_current_context()? {
        if current == name {
            db.set_current_context(None)?;
        }
    }

    println!("{} {}", "✓ Removed from local state:".green(), name.bright_white());
    println!("{}", "  Note: EC2 instance may still exist in AWS".dimmed());

    Ok(())
}
