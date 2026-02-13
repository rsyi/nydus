use crate::config::{Instance, Profile};
use crate::error::{NydusError, Result};
use crate::ssh::{copy_file_to_remote, run_remote_command};
use crate::util;
use std::fs;
use std::path::Path;

/// Sync all credentials to instance
pub fn sync_credentials(instance: &Instance, profile: &Profile) -> Result<()> {
    if !profile.sync_credentials.enabled {
        return Ok(());
    }

    println!("Syncing credentials to {}...", instance.name);

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

    println!("✓ Credential sync complete");

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
