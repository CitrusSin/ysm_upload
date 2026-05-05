use anyhow::Result;
use serde::{Deserialize, Serialize};

use std::fs;
use std::collections::HashMap;
use std::time::Duration;

use crate::oauth::OAuthProviderType;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub oauth: OAuthProvidersConfig,
    pub rcon: RconConfig,
    #[serde(with = "humantime_serde")]
    pub reload_delay: Duration,
    #[serde(default)]
    pub ysm_storage: YsmStorageConfig,
    #[serde(default)]
    pub mcsmanager: MCSManagerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// Collection of OAuth provider configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProvidersConfig {
    /// Prefix URL used to build callback addresses
    pub prefix_url: String,
    /// Secret string used to sign tokens
    pub secret_string: String,
    /// Configuration for each provider
    pub providers: HashMap<String, OAuthProviderConfig>,
}

/// Configuration for a single OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    /// Provider type
    pub provider_type: OAuthProviderType,
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Requested scopes
    pub scopes: Vec<String>,
    /// Whether the provider is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RconConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YsmStorageConfig {
    #[serde(default)]
    pub backend: YsmStorageBackend,
    pub local: Option<LocalStorageConfig>,
    pub sftp: Option<SftpStorageConfig>,
    pub rsync_over_ssh: Option<RsyncOverSshStorageConfig>,
    pub rsync: Option<RsyncStorageConfig>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum YsmStorageBackend {
    #[default]
    MCSManager,
    LocalFile,
    Sftp,
    RsyncOverSsh,
    Rsync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageConfig {
    pub upload_dir: String,
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            upload_dir: default_ysm_upload_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpStorageConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_sftp_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub remote_dir: String,
}

impl Default for SftpStorageConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_sftp_port(),
            username: String::new(),
            remote_dir: default_ysm_upload_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsyncOverSshStorageConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub remote_dir: String,
}

impl Default for RsyncOverSshStorageConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_ssh_port(),
            username: String::new(),
            remote_dir: default_ysm_upload_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsyncStorageConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_rsync_port")]
    pub port: u16,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub remote_dir: String,
}

impl Default for RsyncStorageConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_rsync_port(),
            module: String::new(),
            remote_dir: default_ysm_upload_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCSManagerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub daemon_id: String,
    #[serde(default, alias = "instance_uuid")]
    pub instance_id: String,
    #[serde(default = "default_ysm_upload_dir")]
    pub upload_dir: String,
}

impl Default for MCSManagerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            api_key: String::new(),
            daemon_id: String::new(),
            instance_id: String::new(),
            upload_dir: default_ysm_upload_dir(),
        }
    }
}

impl MCSManagerConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            anyhow::bail!("MCSManager upload is disabled")
        }

        for (field_name, value) in [
            ("mcsmanager.base_url", self.base_url.trim()),
            ("mcsmanager.api_key", self.api_key.trim()),
            ("mcsmanager.daemon_id", self.daemon_id.trim()),
            ("mcsmanager.instance_id", self.instance_id.trim()),
        ] {
            if value.is_empty() {
                anyhow::bail!("{} is not configured", field_name);
            }
        }

        Ok(())
    }
}


fn default_true() -> bool {
    true
}

fn default_ysm_upload_dir() -> String {
    "/config/yes_steve_model/auth".to_string()
}

fn default_sftp_port() -> u16 {
    22
}

fn default_ssh_port() -> u16 {
    22
}

fn default_rsync_port() -> u16 {
    873
}

impl Config {
    fn check_integrity(&self) -> Result<()> {
        match &self.ysm_storage.backend {
            YsmStorageBackend::MCSManager => {
                if let Err(e) = self.mcsmanager.validate() {
                    return Err(e.context("YSM storage backend is set to MCSManager, but MCSManager configuration is invalid"));
                }
            },
            YsmStorageBackend::LocalFile => {
                if self.ysm_storage.local.is_none() {
                    anyhow::bail!("YSM storage backend is set to LocalFile, but local storage configuration is missing");
                }
            },
            YsmStorageBackend::Sftp => {
                if self.ysm_storage.sftp.is_none() {
                    anyhow::bail!("YSM storage backend is set to Sftp, but SFTP storage configuration is missing");
                }
            },
            YsmStorageBackend::Rsync => {
                if self.ysm_storage.rsync.is_none() {
                    anyhow::bail!("YSM storage backend is set to Rsync, but rsync storage configuration is missing");
                }
            },
            YsmStorageBackend::RsyncOverSsh => {
                if self.ysm_storage.rsync_over_ssh.is_none() {
                    anyhow::bail!("YSM storage backend is set to RsyncOverSsh, but rsync-over-ssh storage configuration is missing");
                }
            }
        }
        Ok(())
    }

    /// Load the configuration file
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        config.check_integrity()?;
        Ok(config)
    }

    /// Create a default configuration file
    pub fn create_default(path: &str) -> Result<()> {
        let mut providers = HashMap::new();
        
        // Example BlessingSkin provider configuration
        providers.insert("littleskin".to_string(), OAuthProviderConfig {
            provider_type: OAuthProviderType::LittleSkin,
            client_id: "your_client_id_here".to_string(),
            client_secret: "your_client_secret_here".to_string(),
            scopes: vec![
                "User.Read".to_string(),
                "Player.Read".to_string(),
                "PremiumVerification.Read".to_string(),
            ],
            enabled: true,
        });

        // Example Microsoft provider configuration
        providers.insert("microsoft".to_string(), OAuthProviderConfig {
            provider_type: OAuthProviderType::Microsoft,
            client_id: "your_azure_client_id".to_string(),
            client_secret: "your_client_secret_here".to_string(),
            scopes: vec!["XboxLive.signin".to_string()],
            enabled: false, // Disabled by default
        });

        let default_config = Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            oauth: OAuthProvidersConfig {
                prefix_url: "http://127.0.0.1:3000".to_string(),
                secret_string: "your-secret-here-change-this-in-production".to_string(),
                providers,
            },
            rcon: RconConfig {
                host: "127.0.0.1".to_string(),
                port: 25575,
                password: "your_rcon_password_here".to_string(),
            },
            ysm_storage: YsmStorageConfig::default(),
            mcsmanager: MCSManagerConfig::default(),
            reload_delay: Duration::from_secs(3),
        };

        let yaml = serde_yaml::to_string(&default_config)?;
        fs::write(path, yaml)?;
        Ok(())
    }

}
