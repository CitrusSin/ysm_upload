use anyhow::Result;
use serde::{Deserialize, Serialize};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fs;
use std::collections::HashMap;

use crate::oauth::OAuthProviderType;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub oauth: OAuthProvidersConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// OAuth 提供者配置集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProvidersConfig {
    /// 前缀 URL（用于生成回调地址）
    pub prefix_url: String,
    /// 密钥字符串（用于签名 token）
    pub secret_string: String,
    /// 各个提供者的配置
    pub providers: HashMap<String, OAuthProviderConfig>,
}

/// 单个 OAuth 提供者配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    /// 提供者类型
    pub provider_type: OAuthProviderType,
    /// 客户端 ID
    pub client_id: String,
    /// 客户端密钥
    pub client_secret: String,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Config {
    /// 加载配置文件
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// 创建默认配置文件
    pub fn create_default(path: &str) -> Result<()> {
        let mut providers = HashMap::new();
        
        // BlessingSkin 提供者示例配置
        providers.insert("littleskin".to_string(), OAuthProviderConfig {
            provider_type: OAuthProviderType::BlessingSkin("https://littleskin.cn".to_string()),
            client_id: "your_client_id_here".to_string(),
            client_secret: "your_client_secret_here".to_string(),
            enabled: true,
        });

        // Microsoft 提供者示例配置
        providers.insert("microsoft".to_string(), OAuthProviderConfig {
            provider_type: OAuthProviderType::Microsoft,
            client_id: "your_azure_client_id".to_string(),
            client_secret: "your_azure_client_secret".to_string(),
            enabled: false, // 默认禁用
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
        };

        let yaml = serde_yaml::to_string(&default_config)?;
        fs::write(path, yaml)?;
        Ok(())
    }

    pub fn secret(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(self.oauth.secret_string.as_bytes())
            .expect("HMAC can take key of any size")
    }
}
