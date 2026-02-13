use crate::config::{Instance, Tunnel};
use crate::error::{NydusError, Result};
use std::process::{Command, Stdio};

/// Attach to instance via interactive SSH session
pub fn attach(instance: &Instance) -> Result<()> {
    let ssh_key_path = &instance.ssh_key_path;
    let ssh_user = &instance.ssh_user;
    let host = instance
        .public_ip
        .as_ref()
        .or(instance.public_dns.as_ref())
        .ok_or_else(|| NydusError::SshError("Instance has no public IP or DNS".to_string()))?;

    println!("Connecting to {}@{}...", ssh_user, host);

    let status = Command::new("ssh")
        .arg("-i")
        .arg(ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg(format!("{}@{}", ssh_user, host))
        .status()
        .map_err(|e| NydusError::SshError(format!("Failed to run ssh: {}", e)))?;

    if !status.success() {
        return Err(NydusError::SshError(format!(
            "SSH exited with status: {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

/// Create foreground SSH tunnel (blocking)
pub fn forward_foreground(instance: &Instance, tunnel: &Tunnel) -> Result<()> {
    let ssh_key_path = &instance.ssh_key_path;
    let ssh_user = &instance.ssh_user;
    let host = instance
        .public_ip
        .as_ref()
        .or(instance.public_dns.as_ref())
        .ok_or_else(|| NydusError::SshError("Instance has no public IP or DNS".to_string()))?;

    println!(
        "Forwarding {}:{} -> localhost:{}",
        tunnel.remote_host, tunnel.remote_port, tunnel.local_port
    );
    println!("Press Ctrl+C to stop...");

    let status = Command::new("ssh")
        .arg("-i")
        .arg(ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg("-N")
        .arg("-L")
        .arg(format!(
            "{}:{}:{}",
            tunnel.local_port, tunnel.remote_host, tunnel.remote_port
        ))
        .arg(format!("{}@{}", ssh_user, host))
        .status()
        .map_err(|e| NydusError::SshError(format!("Failed to run ssh: {}", e)))?;

    if !status.success() {
        return Err(NydusError::SshError(format!(
            "SSH tunnel exited with status: {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

/// Create background SSH tunnel, return PID
pub fn forward_background(instance: &Instance, tunnel: &Tunnel) -> Result<i32> {
    let ssh_key_path = &instance.ssh_key_path;
    let ssh_user = &instance.ssh_user;
    let host = instance
        .public_ip
        .as_ref()
        .or(instance.public_dns.as_ref())
        .ok_or_else(|| NydusError::SshError("Instance has no public IP or DNS".to_string()))?;

    println!(
        "Starting background tunnel: {}:{} -> localhost:{}",
        tunnel.remote_host, tunnel.remote_port, tunnel.local_port
    );

    let child = Command::new("ssh")
        .arg("-i")
        .arg(ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg("-N")
        .arg("-f")  // Background mode
        .arg("-L")
        .arg(format!(
            "{}:{}:{}",
            tunnel.local_port, tunnel.remote_host, tunnel.remote_port
        ))
        .arg(format!("{}@{}", ssh_user, host))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| NydusError::SshError(format!("Failed to spawn ssh: {}", e)))?;

    let pid = child.id() as i32;

    // Give SSH a moment to establish connection
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify tunnel is running
    if !crate::util::is_process_alive(pid) {
        return Err(NydusError::SshError("SSH tunnel failed to start".to_string()));
    }

    Ok(pid)
}

/// Stop a tunnel by PID
pub fn stop_tunnel(pid: i32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::process::Command;
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|e| NydusError::SshError(format!("Failed to kill process: {}", e)))?;

        if !status.success() {
            return Err(NydusError::SshError(format!("Failed to stop tunnel (PID {})", pid)));
        }
    }

    #[cfg(not(unix))]
    {
        use std::process::Command;
        let status = Command::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/F")
            .status()
            .map_err(|e| NydusError::SshError(format!("Failed to kill process: {}", e)))?;

        if !status.success() {
            return Err(NydusError::SshError(format!("Failed to stop tunnel (PID {})", pid)));
        }
    }

    Ok(())
}

/// Copy a file to remote instance via SCP
pub fn copy_file_to_remote(
    instance: &Instance,
    local_path: &str,
    remote_path: &str,
    mode: Option<&str>,
) -> Result<()> {
    let ssh_key_path = &instance.ssh_key_path;
    let ssh_user = &instance.ssh_user;
    let host = instance
        .public_ip
        .as_ref()
        .or(instance.public_dns.as_ref())
        .ok_or_else(|| NydusError::SshError("Instance has no public IP or DNS".to_string()))?;

    let status = Command::new("scp")
        .arg("-i")
        .arg(ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg(local_path)
        .arg(format!("{}@{}:{}", ssh_user, host, remote_path))
        .status()
        .map_err(|e| NydusError::SshError(format!("Failed to run scp: {}", e)))?;

    if !status.success() {
        return Err(NydusError::SshError(format!(
            "SCP failed with status: {}",
            status.code().unwrap_or(-1)
        )));
    }

    // Set permissions if specified
    if let Some(mode) = mode {
        run_remote_command(instance, &format!("chmod {} {}", mode, remote_path))?;
    }

    Ok(())
}

/// Run a command on remote instance via SSH
pub fn run_remote_command(instance: &Instance, command: &str) -> Result<String> {
    let ssh_key_path = &instance.ssh_key_path;
    let ssh_user = &instance.ssh_user;
    let host = instance
        .public_ip
        .as_ref()
        .or(instance.public_dns.as_ref())
        .ok_or_else(|| NydusError::SshError("Instance has no public IP or DNS".to_string()))?;

    let output = Command::new("ssh")
        .arg("-i")
        .arg(ssh_key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg(format!("{}@{}", ssh_user, host))
        .arg(command)
        .output()
        .map_err(|e| NydusError::SshError(format!("Failed to run ssh: {}", e)))?;

    if !output.status.success() {
        return Err(NydusError::SshError(format!(
            "Remote command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
