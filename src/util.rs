use crate::error::Result;
use std::path::PathBuf;

/// Check if a process is alive
pub fn is_process_alive(pid: i32) -> bool {
    // Use kill with signal 0 to check if process exists
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        // Windows: use tasklist
        use std::process::Command;
        Command::new("tasklist")
            .arg("/FI")
            .arg(format!("PID eq {}", pid))
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
}

/// Expand tilde in path
pub fn expand_tilde(path: &str) -> Result<PathBuf> {
    if path.starts_with("~/") {
        let home = home::home_dir()
            .ok_or_else(|| crate::error::NydusError::ConfigError("Could not find home directory".to_string()))?;
        Ok(home.join(&path[2..]))
    } else if path == "~" {
        home::home_dir()
            .ok_or_else(|| crate::error::NydusError::ConfigError("Could not find home directory".to_string()))
    } else {
        Ok(PathBuf::from(path))
    }
}

/// Format duration in human-readable form
pub fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

/// Open browser to URL
pub fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| crate::error::NydusError::IoError(e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| crate::error::NydusError::IoError(e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/C", "start", url])
            .spawn()
            .map_err(|e| crate::error::NydusError::IoError(e))?;
    }

    Ok(())
}

/// Find an available local port starting from the given port
pub fn find_available_port(start: u16) -> Result<u16> {
    for port in start..65535 {
        if is_port_available(port) {
            return Ok(port);
        }
    }
    Err(crate::error::NydusError::TunnelError("No available ports".to_string()))
}

/// Check if a port is available
fn is_port_available(port: u16) -> bool {
    use std::net::TcpListener;
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Interpolate environment variables in a string (${VAR} syntax)
pub fn interpolate_env_vars(value: &str) -> String {
    let mut result = value.to_string();

    // Find all ${VAR} patterns and replace them
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result.replace_range(start..start + end + 1, &replacement);
        } else {
            break;
        }
    }

    result
}

/// Get the current timestamp in seconds
pub fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
