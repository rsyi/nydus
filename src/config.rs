use crate::error::{NydusError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Get the nydus directory path (~/.nydus)
pub fn nydus_dir() -> Result<PathBuf> {
    let home = home::home_dir().ok_or_else(|| NydusError::ConfigError("Could not find home directory".to_string()))?;
    Ok(home.join(".nydus"))
}

/// Get the config file path (~/.nydus/config.yml)
pub fn config_path() -> Result<PathBuf> {
    Ok(nydus_dir()?.join("config.yml"))
}

/// Get the state database path (~/.nydus/state.sqlite)
pub fn state_db_path() -> Result<PathBuf> {
    Ok(nydus_dir()?.join("state.sqlite"))
}

/// Initialize the nydus directory and create default config
pub fn init_nydus_dir() -> Result<()> {
    let dir = nydus_dir()?;

    // Create directory if it doesn't exist
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    // Create default config if it doesn't exist
    let config_file = config_path()?;
    if !config_file.exists() {
        let default_config = Config::default();
        default_config.save()?;
    }

    // Initialize state database
    let db_path = state_db_path()?;
    crate::state::StateDb::open(&db_path)?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub profiles: Vec<Profile>,
}

impl Config {
    /// Load config from file
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| NydusError::ConfigError(format!("Failed to parse config: {}", e)))
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        let content = serde_yaml::to_string(self)
            .map_err(|e| NydusError::ConfigError(format!("Failed to serialize config: {}", e)))?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Get a profile by name
    pub fn get_profile(&self, name: &str) -> Result<&Profile> {
        self.profiles
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| NydusError::ProfileNotFound(name.to_string()))
    }

    /// Add or update a profile
    pub fn upsert_profile(&mut self, profile: Profile) {
        if let Some(existing) = self.profiles.iter_mut().find(|p| p.name == profile.name) {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            profiles: vec![Profile::default()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub region: String,
    pub instance_type: String,
    pub ami: Option<String>,  // If None, use latest Ubuntu
    pub ssh_user: String,
    pub ssh_key_path: String,
    pub security_group: Option<String>,  // If None, create default
    pub subnet_id: Option<String>,
    pub volume_size_gb: u32,
    pub tags: HashMap<String, String>,
    #[serde(default)]
    pub sync_credentials: SyncCredentials,
    #[serde(default)]
    pub tools: ToolsConfig,
}

impl Default for Profile {
    fn default() -> Self {
        let mut tags = HashMap::new();
        tags.insert("Environment".to_string(), "development".to_string());
        tags.insert("ManagedBy".to_string(), "nydus".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("EDITOR".to_string(), "vim".to_string());
        env_vars.insert("NODE_ENV".to_string(), "development".to_string());

        Profile {
            name: "default".to_string(),
            region: "us-east-1".to_string(),
            instance_type: "t3.xlarge".to_string(),
            ami: None,  // Will auto-resolve to latest Ubuntu
            ssh_user: "ubuntu".to_string(),
            ssh_key_path: "~/.ssh/nydus-key.pem".to_string(),
            security_group: None,
            subnet_id: None,
            volume_size_gb: 30,
            tags,
            sync_credentials: SyncCredentials {
                enabled: true,
                git: GitSync {
                    ssh_keys: vec![],  // Don't sync keys - use agent forwarding instead!
                    config: true,
                    gpg_keys: false,
                    repositories: vec![],
                },
                claude: ClaudeSync {
                    enabled: true,
                    session_file: Some("~/.claude/session_token".to_string()),
                    api_key_env: Some("CLAUDE_API_KEY".to_string()),
                },
                aws: AwsSync {
                    enabled: false,
                },
                env_vars,
                dotfiles: vec!["~/.vimrc".to_string()],
            },
            tools: ToolsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncCredentials {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub git: GitSync,
    #[serde(default)]
    pub claude: ClaudeSync,
    #[serde(default)]
    pub aws: AwsSync,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    #[serde(default)]
    pub dotfiles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitSync {
    #[serde(default)]
    pub ssh_keys: Vec<String>,
    #[serde(default)]
    pub config: bool,
    #[serde(default)]
    pub gpg_keys: bool,
    #[serde(default)]
    pub repositories: Vec<RepositoryConfig>,  // Git repos to clone
}

/// Repository configuration - can be a plain URL string or a struct with setup_commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepositoryConfig {
    Url(String),
    Full {
        url: String,
        #[serde(default)]
        setup_commands: Vec<String>,
    },
}

impl RepositoryConfig {
    pub fn url(&self) -> &str {
        match self {
            RepositoryConfig::Url(url) => url,
            RepositoryConfig::Full { url, .. } => url,
        }
    }

    pub fn setup_commands(&self) -> &[String] {
        match self {
            RepositoryConfig::Url(_) => &[],
            RepositoryConfig::Full { setup_commands, .. } => setup_commands,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeSync {
    #[serde(default)]
    pub enabled: bool,
    pub session_file: Option<String>,
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AwsSync {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub presets: ToolPresets,
    #[serde(default)]
    pub languages: LanguagesConfig,
    #[serde(default)]
    pub custom_apt: Vec<String>,
    #[serde(default)]
    pub custom_mise: Vec<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        ToolsConfig {
            presets: ToolPresets {
                essentials: true,
                dev_tools: true,
                editors: true,
            },
            languages: LanguagesConfig {
                rust: Some(true),
                node: Some("20".to_string()),
                pnpm: Some(true),
                python: None,
                go: None,
                deno: None,
            },
            custom_apt: vec![],
            custom_mise: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPresets {
    #[serde(default)]
    pub essentials: bool,  // tmux, curl, wget, git, unzip
    #[serde(default)]
    pub dev_tools: bool,   // htop, ripgrep, jq, tree
    #[serde(default)]
    pub editors: bool,     // neovim (from official binary)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguagesConfig {
    pub rust: Option<bool>,
    pub node: Option<String>,  // Version or "true" for latest
    pub pnpm: Option<bool>,
    pub python: Option<String>,
    pub go: Option<String>,
    pub deno: Option<bool>,
}

/// Instance represents an EC2 instance tracked by nydus
#[derive(Debug, Clone)]
pub struct Instance {
    pub id: Option<i64>,  // Database ID
    pub name: String,
    pub profile: String,
    pub region: String,
    pub instance_id: String,
    pub public_ip: Option<String>,
    pub public_dns: Option<String>,
    pub private_ip: Option<String>,
    pub ssh_user: String,
    pub ssh_key_path: String,
    pub created_at: i64,  // Unix timestamp
    pub last_seen: i64,  // Unix timestamp
    pub last_synced: Option<i64>,  // Unix timestamp
    pub desired_state: String,  // 'running' or 'stopped'
    pub status: Option<String>,  // Cached EC2 status
    pub notes: Option<String>,
}

/// Tunnel represents an SSH port forward
#[derive(Debug, Clone)]
pub struct Tunnel {
    pub id: Option<i64>,  // Database ID
    pub instance_name: String,
    pub remote_host: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub pid: Option<i32>,
    pub started_at: i64,  // Unix timestamp
    pub mode: String,  // 'foreground' or 'background'
    pub open_url: bool,
}
