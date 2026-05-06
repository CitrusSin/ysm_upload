use std::path::Path;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sha2::Sha256;
use tracing::{info, warn, error};
use crate::config::{self, Config, OAuthProviderConfig};
use hmac::{Hmac, Mac};


const CONFIG_FILE: &str = "config.yml";

pub struct AppState {
    pub config: Config,
    
    secret_key: Hmac<Sha256>
}

impl AppState {
    pub fn new() -> Self {
        // Check whether the configuration file exists
        if !Path::new(CONFIG_FILE).exists() {
            warn!("Configuration file not found, creating a default one...");
            
            match config::Config::create_default(CONFIG_FILE) {
                Ok(_) => {
                    info!("Created default configuration file: {}", CONFIG_FILE);
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
        let app_config = match config::Config::load(CONFIG_FILE) {
            Ok(config) => {
                info!("Configuration file loaded successfully: {}", CONFIG_FILE);
                config
            }
            Err(e) => {
                error!("Failed to load configuration file: {:?}", e);
                std::process::exit(1);
            }
        };

        let secret_key = Hmac::<Sha256>::new_from_slice(app_config.oauth.secret_string.as_bytes())
            .expect("HMAC can take key of any size");

        AppState { config: app_config, secret_key }
    }


    /// Get the redirect URL
    pub fn get_redirect_uri(&self, provider: &str) -> String {
        format!("{}/api/oauth/{}/callback", self.config.oauth.prefix_url, provider)
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