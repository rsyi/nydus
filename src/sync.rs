use crate::config::{Instance, Profile};
use crate::error::{NydusError, Result};
use crate::ssh::{copy_file_to_remote, run_remote_command};
use crate::util;
use colored::*;
use std::path::Path;
use std::time::Instant;

/// Sync all credentials to instance
pub fn sync_credentials(instance: &Instance, profile: &Profile) -> Result<()> {
    if !profile.sync_credentials.enabled {
        return Ok(());
    }

    let start_time = Instant::now();
    print!("⏱  Syncing credentials... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    // Install essential tools
    install_essential_tools(instance)?;

    // Install mise and dev tools (rust, pnpm)
    install_mise_tools(instance)?;

    // Install Claude Code
    install_claude_code(instance)?;

    // Sync git credentials
    if !profile.sync_credentials.git.ssh_keys.is_empty()
        || profile.sync_credentials.git.config
        || profile.sync_credentials.git.gpg_keys
    {
        sync_git_credentials(instance, profile)?;
    }

    // Sync Claude credentials
    if profile.sync_credentials.claude.enabled {
        sync_claude_credentials(instance, profile)?;
    }

    // Sync AWS credentials
    if profile.sync_credentials.aws.enabled {
        sync_aws_credentials(instance)?;
    }

    // Sync environment variables
    if !profile.sync_credentials.env_vars.is_empty() {
        sync_env_vars(instance, &profile.sync_credentials.env_vars)?;
    }

    // Sync dotfiles
    if !profile.sync_credentials.dotfiles.is_empty() {
        sync_dotfiles(instance, &profile.sync_credentials.dotfiles)?;
    }

    // Auto-sync common dev configs if they exist
    sync_dev_configs(instance)?;

    let elapsed = start_time.elapsed();
    let elapsed_str = format!("{:.1}s", elapsed.as_secs_f64());
    println!("✓ Sync complete! {}", format!("({})", elapsed_str).truecolor(255, 165, 0).bold());

    Ok(())
}

/// Sync git credentials (SSH keys, config, GPG keys)
fn sync_git_credentials(instance: &Instance, profile: &Profile) -> Result<()> {
    println!("  → Syncing git credentials...");

    // Ensure .ssh directory exists with correct permissions
    run_remote_command(instance, "mkdir -p ~/.ssh && chmod 700 ~/.ssh")?;

    // Copy SSH keys
    for key_path in &profile.sync_credentials.git.ssh_keys {
        let expanded = util::expand_tilde(key_path)?;
        let expanded_str = expanded.to_string_lossy();

        if !expanded.exists() {
            eprintln!("    ⚠ SSH key not found: {}", key_path);
            continue;
        }

        let key_name = Path::new(key_path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| NydusError::SyncError(format!("Invalid key path: {}", key_path)))?;

        println!("    → Copying SSH key: {}", key_name);
        copy_file_to_remote(instance, &expanded_str, &format!("~/.ssh/{}", key_name), Some("600"))?;

        // Also copy .pub file if it exists
        let pub_key_path = format!("{}.pub", expanded_str);
        if Path::new(&pub_key_path).exists() {
            copy_file_to_remote(
                instance,
                &pub_key_path,
                &format!("~/.ssh/{}.pub", key_name),
                Some("644"),
            )?;
        }
    }

    // Copy gitconfig
    if profile.sync_credentials.git.config {
        let gitconfig_path = util::expand_tilde("~/.gitconfig")?;
        if gitconfig_path.exists() {
            println!("    → Copying .gitconfig");
            copy_file_to_remote(
                instance,
                &gitconfig_path.to_string_lossy(),
                "~/.gitconfig",
                Some("644"),
            )?;
        }
    }

    // Copy GPG keys
    if profile.sync_credentials.git.gpg_keys {
        let gnupg_path = util::expand_tilde("~/.gnupg")?;
        if gnupg_path.exists() {
            println!("    → Copying GPG keys");
            // Create .gnupg directory
            run_remote_command(instance, "mkdir -p ~/.gnupg && chmod 700 ~/.gnupg")?;

            // Export and import GPG keys
            // This is complex - for now, just copy the whole directory
            // TODO: Use gpg --export/--import for proper key transfer
            eprintln!("    ⚠ GPG key sync not fully implemented yet");
        }
    }

    // Start ssh-agent and add keys
    if !profile.sync_credentials.git.ssh_keys.is_empty() {
        setup_ssh_agent_on_remote(instance, &profile.sync_credentials.git.ssh_keys)?;
    }

    Ok(())
}

/// Sync Claude credentials
fn sync_claude_credentials(instance: &Instance, profile: &Profile) -> Result<()> {
    println!("  → Syncing Claude credentials...");

    // Ensure .claude directory exists
    run_remote_command(instance, "mkdir -p ~/.claude")?;

    // Copy session file if specified
    if let Some(session_file) = &profile.sync_credentials.claude.session_file {
        let expanded = util::expand_tilde(session_file)?;
        if expanded.exists() {
            println!("    → Copying Claude session token");
            copy_file_to_remote(
                instance,
                &expanded.to_string_lossy(),
                "~/.claude/session_token",
                Some("600"),
            )?;
        }
    }

    // Set API key environment variable
    if let Some(api_key_env) = &profile.sync_credentials.claude.api_key_env {
        if let Ok(api_key) = std::env::var(api_key_env) {
            println!("    → Setting Claude API key env var");
            // Append to .bashrc
            let export_line = format!("export {}='{}'", api_key_env, api_key);
            run_remote_command(
                instance,
                &format!("echo \"{}\" >> ~/.bashrc", export_line),
            )?;
        }
    }

    Ok(())
}

/// Sync AWS credentials
fn sync_aws_credentials(instance: &Instance) -> Result<()> {
    println!("  → Syncing AWS credentials...");

    let aws_dir = util::expand_tilde("~/.aws")?;
    if !aws_dir.exists() {
        return Ok(());
    }

    // Ensure .aws directory exists
    run_remote_command(instance, "mkdir -p ~/.aws && chmod 700 ~/.aws")?;

    // Copy credentials file
    let credentials_path = aws_dir.join("credentials");
    if credentials_path.exists() {
        println!("    → Copying AWS credentials");
        copy_file_to_remote(
            instance,
            &credentials_path.to_string_lossy(),
            "~/.aws/credentials",
            Some("600"),
        )?;
    }

    // Copy config file
    let config_path = aws_dir.join("config");
    if config_path.exists() {
        println!("    → Copying AWS config");
        copy_file_to_remote(
            instance,
            &config_path.to_string_lossy(),
            "~/.aws/config",
            Some("600"),
        )?;
    }

    Ok(())
}

/// Sync environment variables
fn sync_env_vars(instance: &Instance, env_vars: &std::collections::HashMap<String, String>) -> Result<()> {
    println!("  → Syncing environment variables...");

    for (key, value) in env_vars {
        // Interpolate ${VAR} syntax
        let interpolated_value = util::interpolate_env_vars(value);

        println!("    → Setting {}", key);
        let export_line = format!("export {}='{}'", key, interpolated_value);
        run_remote_command(
            instance,
            &format!("grep -q \"export {}=\" ~/.bashrc || echo \"{}\" >> ~/.bashrc", key, export_line),
        )?;
    }

    Ok(())
}

/// Sync dotfiles
fn sync_dotfiles(instance: &Instance, dotfiles: &[String]) -> Result<()> {
    println!("  → Syncing dotfiles...");

    for dotfile in dotfiles {
        let expanded = util::expand_tilde(dotfile)?;
        if !expanded.exists() {
            eprintln!("    ⚠ Dotfile not found: {}", dotfile);
            continue;
        }

        let filename = Path::new(dotfile)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| NydusError::SyncError(format!("Invalid dotfile path: {}", dotfile)))?;

        println!("    → Copying {}", filename);
        copy_file_to_remote(
            instance,
            &expanded.to_string_lossy(),
            &format!("~/{}", filename),
            Some("644"),
        )?;
    }

    Ok(())
}

/// Auto-sync common dev configs if they exist locally
fn sync_dev_configs(instance: &Instance) -> Result<()> {
    println!("  → Checking for dev configs...");

    // Sync ~/.config/nvim directory if it exists
    let nvim_config_dir = util::expand_tilde("~/.config/nvim")?;
    if nvim_config_dir.exists() && nvim_config_dir.is_dir() {
        println!("    → Syncing nvim config");

        // Create ~/.config/nvim directory on remote
        run_remote_command(instance, "mkdir -p ~/.config/nvim")?;

        // Use rsync via SSH to copy the entire directory
        let ssh_key_path = &instance.ssh_key_path;
        let ssh_user = &instance.ssh_user;
        let host = instance
            .public_ip
            .as_ref()
            .or(instance.public_dns.as_ref())
            .ok_or_else(|| NydusError::SyncError("Instance has no public IP or DNS".to_string()))?;

        let rsync_result = std::process::Command::new("rsync")
            .arg("-az")
            .arg("-e")
            .arg(format!("ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null", ssh_key_path))
            .arg(format!("{}/.config/nvim/", nvim_config_dir.to_string_lossy()))
            .arg(format!("{}@{}:~/.config/nvim/", ssh_user, host))
            .status();

        match rsync_result {
            Ok(status) if status.success() => {
                println!("{}", "    ✓ nvim config synced".green());
            }
            _ => {
                eprintln!("    ⚠ Failed to sync nvim config");
            }
        }
    }

    // Sync .tmux.conf if it exists
    let tmux_conf = util::expand_tilde("~/.tmux.conf")?;
    if tmux_conf.exists() {
        println!("    → Copying .tmux.conf");
        copy_file_to_remote(
            instance,
            &tmux_conf.to_string_lossy(),
            "~/.tmux.conf",
            Some("644"),
        )?;
        println!("{}", "    ✓ .tmux.conf synced".green());
    }

    Ok(())
}

/// Setup ssh-agent on remote and add keys
fn setup_ssh_agent_on_remote(instance: &Instance, ssh_keys: &[String]) -> Result<()> {
    println!("    → Setting up ssh-agent");

    // Create script to start ssh-agent if not running
    let script = r#"
if [ -z "$SSH_AUTH_SOCK" ]; then
    eval $(ssh-agent -s) > /dev/null 2>&1
    echo "export SSH_AUTH_SOCK=$SSH_AUTH_SOCK" >> ~/.bashrc
    echo "export SSH_AGENT_PID=$SSH_AGENT_PID" >> ~/.bashrc
fi
"#;

    run_remote_command(instance, script)?;

    // Add keys to agent
    for key_path in ssh_keys {
        let key_name = Path::new(key_path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| NydusError::SyncError(format!("Invalid key path: {}", key_path)))?;

        run_remote_command(
            instance,
            &format!("eval $(ssh-agent -s) > /dev/null 2>&1 && ssh-add ~/.ssh/{} 2>/dev/null || true", key_name),
        )?;
    }

    Ok(())
}

/// Install essential tools (tmux, nvim, etc.)
fn install_essential_tools(instance: &Instance) -> Result<()> {
    let start_time = Instant::now();
    println!("  → Installing essential tools...");

    // Update package lists
    let update_script = "sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq 2>&1";
    run_remote_command(instance, update_script).ok();

    // Install basic tools via apt
    let packages = vec![
        ("tmux", "tmux"),
        ("curl", "curl"),
        ("wget", "wget"),
        ("git", "git"),
        ("unzip", "unzip"),
    ];

    let mut installed = vec![];
    let mut failed = vec![];

    for (name, package) in packages {
        let install_cmd = format!("sudo DEBIAN_FRONTEND=noninteractive apt-get install -y {} -qq 2>&1", package);
        match run_remote_command(instance, &install_cmd) {
            Ok(_) => installed.push(name),
            Err(_) => failed.push(name),
        }
    }

    // Install neovim from official binary
    let nvim_install = r#"
curl -LO https://github.com/neovim/neovim/releases/latest/download/nvim-linux-x86_64.tar.gz 2>&1
sudo rm -rf /opt/nvim-linux-x86_64
sudo tar -C /opt -xzf nvim-linux-x86_64.tar.gz
rm nvim-linux-x86_64.tar.gz

# Add to PATH
if ! grep -q '/opt/nvim-linux-x86_64/bin' ~/.bashrc 2>/dev/null; then
    echo 'export PATH="/opt/nvim-linux-x86_64/bin:$PATH"' >> ~/.bashrc
fi

# Verify installation
if [ -f /opt/nvim-linux-x86_64/bin/nvim ]; then
    echo "SUCCESS"
else
    echo "FAILED"
    exit 1
fi
"#;

    match run_remote_command(instance, nvim_install) {
        Ok(output) if output.contains("SUCCESS") => {
            installed.push("neovim");
        }
        _ => {
            failed.push("neovim");
        }
    }

    let final_elapsed = start_time.elapsed();
    let final_time = format!("{:.1}s", final_elapsed.as_secs_f64());

    if !installed.is_empty() {
        print!("    ✓ Installed: {} ", installed.join(", "));
        println!("{}", format!("({})", final_time).dimmed());
    }
    if !failed.is_empty() {
        eprintln!("    ⚠ Failed: {}", failed.join(", "));
    }

    Ok(())
}

/// Install mise and dev tools (rust, pnpm)
fn install_mise_tools(instance: &Instance) -> Result<()> {
    let start_time = Instant::now();
    println!("  → Installing mise & dev tools...");

    // Install mise
    let mise_install = r#"
# Install mise
curl https://mise.run | sh

# Add mise to PATH and enable it
if ! grep -q 'mise activate' ~/.bashrc 2>/dev/null; then
    echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc
fi

# Source it for current session
eval "$(~/.local/bin/mise activate bash)" || true

# Verify mise is available
if command -v mise >/dev/null 2>&1 || [ -f ~/.local/bin/mise ]; then
    echo "MISE_OK"
else
    echo "MISE_FAILED"
    exit 1
fi
"#;

    let mut installed = vec![];
    let mut failed = vec![];

    match run_remote_command(instance, mise_install) {
        Ok(output) if output.contains("MISE_OK") => {
            // Mise installed, now install rust and pnpm
            let tools = vec![
                ("rust", "rust"),
                ("pnpm", "pnpm"),
            ];

            for (name, tool) in tools {
                let install_cmd = format!(
                    "eval \"$(~/.local/bin/mise activate bash)\" && ~/.local/bin/mise install {} && ~/.local/bin/mise use -g {}",
                    tool, tool
                );
                match run_remote_command(instance, &install_cmd) {
                    Ok(_) => installed.push(name),
                    Err(_) => failed.push(name),
                }
            }
        }
        _ => {
            failed.push("mise");
        }
    }

    let final_elapsed = start_time.elapsed();
    let final_time = format!("{:.1}s", final_elapsed.as_secs_f64());

    if !installed.is_empty() {
        print!("    ✓ Installed via mise: {} ", installed.join(", "));
        println!("{}", format!("({})", final_time).dimmed());
    }
    if !failed.is_empty() {
        eprintln!("    ⚠ Failed: {}", failed.join(", "));
    }

    Ok(())
}

/// Install Claude Code
fn install_claude_code(instance: &Instance) -> Result<()> {
    println!("  → Installing Claude Code...");

    // Check if claude is already installed
    let check_result = run_remote_command(instance, "bash -lc 'which claude 2>/dev/null'");
    if check_result.is_ok() {
        println!("    ✓ Claude Code already installed");
        return Ok(());
    }

    // Install Claude Code using the official installer
    let install_script = r#"
set -e

# Run the official Claude installer
curl -fsSL https://claude.ai/install.sh | bash

# Verify installation
if command -v claude >/dev/null 2>&1 || [ -f "$HOME/.local/bin/claude" ]; then
    echo "SUCCESS: Claude Code installed"
else
    echo "ERROR: Installation completed but claude not found"
    exit 1
fi
"#;

    match run_remote_command(instance, install_script) {
        Ok(output) => {
            if output.contains("SUCCESS") {
                println!("{}", "    ✓ Claude Code installed".green());
            } else {
                println!("{}", "    ⚠ Installation uncertain".yellow());
            }
        }
        Err(e) => {
            println!("{}", "    ⚠ Claude Code installation failed".yellow());
            println!("    Install manually after connecting:");
            println!("    {}", "curl -fsSL https://claude.ai/install.sh | bash".bright_white());
        }
    }

    Ok(())
}
