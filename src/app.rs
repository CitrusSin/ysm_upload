use std::path::Path;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sha2::Sha256;
use tracing::{info, warn, error};
use crate::config::{self, Config, OAuthProviderConfig};
use hmac::{Hmac, Mac};
use reqwest::Url;


pub struct AppState {
    pub config: Config,

    secret_key: Hmac<Sha256>,
    frontend_base_path: String,
}

impl AppState {
    pub fn new(config_path: &Path) -> Self {
        // Check whether the configuration file exists
        if !config_path.exists() {
            warn!("Configuration file not found, creating a default one...");
            
            match config::Config::create_default(config_path) {
                Ok(_) => {
                    info!("Created default configuration file: {}", config_path.display());
                    info!("Please update the configuration and run the program again");
                    std::process::exit(0);
                }
                Err(e) => {
                    error!("Failed to create configuration file: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        // Load the configuration file
        let app_config = match config::Config::load(config_path) {
            Ok(config) => {
                info!("Configuration file loaded successfully: {}", config_path.display());
                config
            }
            Err(e) => {
                error!("Failed to load configuration file: {:?}", e);
                std::process::exit(1);
            }
        };

        let secret_key = Hmac::<Sha256>::new_from_slice(app_config.oauth.secret_string.as_bytes())
            .expect("HMAC can take key of any size");

        let frontend_base_path = Url::parse(&app_config.server.prefix_url)
            .ok()
            .map(|url| normalize_base_path(url.path()))
            .unwrap_or_else(|| "/".to_string());

        AppState { config: app_config, secret_key, frontend_base_path }
    }


    /// Get the redirect URL
    pub fn get_redirect_uri(&self, provider: &str) -> String {
        format!("{}/api/oauth/{}/callback", self.public_base_url(), provider)
    }

    pub fn public_base_url(&self) -> &str {
        self.config.server.prefix_url.trim_end_matches('/')
    }

    pub fn frontend_base_path(&self) -> &str {
        &self.frontend_base_path
    }

    pub fn frontend_base_href(&self) -> String {
        if self.frontend_base_path == "/" {
            "/".to_string()
        } else {
            format!("{}/", self.frontend_base_path)
        }
    }

    pub fn app_path(&self, path: &str) -> String {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        if self.frontend_base_path == "/" {
            normalized_path
        } else {
            format!("{}{}", self.frontend_base_path, normalized_path)
        }
    }

    /// Get all enabled providers
    pub fn get_enabled_providers(&self) -> Vec<(String, &OAuthProviderConfig)> {
        self.config.oauth.providers
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, config)| (name.clone(), config))
            .collect()
    }

    /// Get a specific provider configuration
    pub fn get_provider(&self, name: &str) -> Option<&OAuthProviderConfig> {
        self.config.oauth.providers.get(name)
    }

    pub fn secret(&self) -> &Hmac<Sha256> {
        &self.secret_key
    }
}

fn normalize_base_path(path: &str) -> String {
    let trimmed = path.trim();

    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }

    format!("/{}", trimmed.trim_matches('/'))
}


pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError(pub anyhow::Error);

impl AsRef<anyhow::Error> for AppError {
    fn as_ref(&self) -> &anyhow::Error {
        &self.0
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(error = ?self.0, "Request failed");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}