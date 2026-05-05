use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use async_trait::async_trait;
use anyhow::{Error, Result};

use tracing::debug;
use uuid::Uuid;

use crate::{config::OAuthProviderConfig, oauth::{OAuthProvider, OAuthProviderType, UnifiedUserInfo, YggdrasilProfile}};


pub struct MicrosoftProvider {
    config: OAuthProviderConfig,
    name: String
}

impl MicrosoftProvider {
    pub fn new(config: OAuthProviderConfig, name: String) -> Self {
        MicrosoftProvider { config, name }
    }
}

#[async_trait]
impl OAuthProvider for MicrosoftProvider {
    fn get_authorize_url(&self, redirect_uri: &str, state: &str) -> String {
        let client_id = &self.config.client_id;
        let scopes = self.config.scopes.join(" ");
        format!(
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}&response_mode=query",
            client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(state)
        )
    }

    async fn exchange_token(&self, code: &str, redirect_uri: &str) -> Result<(String, Duration)> {
        let client = reqwest::Client::new();

        let token_data: HashMap<String, serde_json::Value> = client.post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &self.config.client_id),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_secret", &self.config.client_secret)
            ])
            .send().await?.error_for_status()?
            .json().await?;
        let access_token = token_data
            .get("access_token")
            .ok_or_else(|| Error::msg("OAuth2 access_token field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("OAuth2 access_token is not a string"))?
            .to_string();
        
        let xl_resp: HashMap<String, serde_json::Value> = client
            .post("https://user.auth.xboxlive.com/user/authenticate")
            .json(&serde_json::json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={}", access_token)
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            }))
            .send().await?.error_for_status()?
            .json().await?;

        let xl_token = xl_resp
            .get("Token")
            .ok_or_else(|| Error::msg("XboxLive Token field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("XboxLive Token is not a string"))?;

        let xsts_resp: HashMap<String, serde_json::Value> = client
            .post("https://xsts.auth.xboxlive.com/xsts/authorize")
            .json(&serde_json::json!({
                "Properties": {
                    "SandboxId": "RETAIL",
                    "UserTokens": [xl_token]
                },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            }))
            .send().await?.error_for_status()?
            .json().await?;

        let xsts_token = xsts_resp
            .get("Token")
            .ok_or_else(|| Error::msg("XSTS Token field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("XSTS Token is not a string"))?;
        let user_hash = xsts_resp
            .get("DisplayClaims")
            .ok_or_else(|| Error::msg("XSTS DisplayClaims field does not exist"))?
            .as_object()
            .ok_or_else(|| Error::msg("XSTS DisplayClaims field is not an object"))?
            .get("xui")
            .ok_or_else(|| Error::msg("XSTS xui field does not exist"))?
            .as_array()
            .ok_or_else(|| Error::msg("XSTS xui field is not an array"))?
            .get(0)
            .ok_or_else(|| Error::msg("XSTS xui array does not contain an item"))?
            .as_object()
            .ok_or_else(|| Error::msg("XSTS xui array item is not an object"))?
            .get("uhs")
            .ok_or_else(|| Error::msg("XSTS uhs field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("XSTS uhs field is not a string"))?;

        debug!("Get user hash: {user_hash}, xsts_token: {xsts_token}");

        let xbl_auth = format!("XBL3.0 x={};{}", user_hash, xsts_token);
        let minecraft_token_resp: HashMap<String, serde_json::Value> = client
            .post("https://api.minecraftservices.com/authentication/login_with_xbox")
            .header("Authorization", &xbl_auth)
            .json(&serde_json::json!({
                "identityToken": &xbl_auth
            }))
            .send().await?.error_for_status()?
            .json().await?;

        let minecraft_token = minecraft_token_resp
            .get("access_token")
            .ok_or_else(|| Error::msg("MinecraftServices access_token field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("MinecraftService access_token field is not a string"))?;

        let expires_in = minecraft_token_resp
            .get("expires_in")
            .ok_or_else(|| Error::msg("MinecraftServices expires_in field does not exist"))?
            .as_u64()
            .ok_or_else(|| Error::msg("MinecraftService expires_in field is not a number"))?;

        Ok((minecraft_token.to_string(), Duration::from_secs(expires_in)))
    }

    async fn get_user_info(&self, access_token: &str) -> Result<UnifiedUserInfo> {
        let client = reqwest::Client::new();
        
        let resp: HashMap<String, serde_json::Value> = client
            .get("https://api.minecraftservices.com/minecraft/profile")
            .bearer_auth(access_token)
            .send().await?.error_for_status()?
            .json().await?;

        let uuid = resp
            .get("id")
            .ok_or_else(|| Error::msg("MinecraftService id field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("Minecraft Service id field is not a string"))?;
        let uuid = Uuid::from_str(uuid)?;

        let player_name = resp
            .get("name")
            .ok_or_else(|| Error::msg("MinecraftService id field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("Minecraft Service id field is not a string"))?;

        return Ok(UnifiedUserInfo {
            nickname: player_name.to_string(),
            provider: self.name.clone(),
            provider_type: self.provider_type(),
            premium_verification: None,
            profiles: vec![YggdrasilProfile {
                id: uuid,
                name: player_name.to_string(),
                properties: vec![]
            }]
        })
    }

    fn provider_type(&self) -> OAuthProviderType {
        OAuthProviderType::Microsoft
    }
}