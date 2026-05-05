use anyhow::Result;
use serde::{Deserialize, Serialize};

use std::fs;
use std::collections::HashMap;

use crate::oauth::OAuthProviderType;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub oauth: OAuthProvidersConfig,
    pub rcon: RconConfig,
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


fn default_true() -> bool {
    true
}

impl Config {
    /// Load the configuration file
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Create a default configuration file
    pub fn create_default(path: &str) -> Result<()> {
        let mut providers = HashMap::new();
        
        // Example BlessingSkin provider configuration
        providers.insert("littleskin".to_string(), OAuthProviderConfig {
            provider_type: OAuthProviderType::BlessingSkin("https://littleskin.cn".to_string()),
            client_id: "your_client_id_here".to_string(),
            client_secret: "your_client_secret_here".to_string(),
            scopes: vec!["User.Read".to_string(), "Players.Read".to_string()],
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
        };

        let yaml = serde_yaml::to_string(&default_config)?;
        fs::write(path, yaml)?;
        Ok(())
    }

}
