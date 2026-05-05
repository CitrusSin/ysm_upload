use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::{debug, warn};
use uuid::Uuid;

use super::{OAuthProvider, OAuthProviderType, PremiumVerificationStatus, UnifiedUserInfo};
use crate::{
    config::OAuthProviderConfig,
    external_api::{mojang::get_profile_from_uuid, YggdrasilProfile},
};

#[derive(Deserialize, Debug, Clone)]
struct LittleSkinUserInfo {
    nickname: String,
}

#[derive(Deserialize, Debug, Clone)]
struct LittleSkinPlayer {
    name: String,
}

#[derive(Deserialize, Debug, Clone)]
struct PremiumVerificationVerified {
    verified: bool,
    uuid: Uuid,
}

#[derive(Deserialize, Debug, Clone)]
struct PremiumVerificationNotVerified {
    verified: bool,
}

pub struct LittleSkinProvider {
    config: OAuthProviderConfig,
    name: String,
}

impl LittleSkinProvider {
    pub fn new(config: OAuthProviderConfig, name: String) -> Self {
        Self { config, name }
    }

    fn base_url(&self) -> &str {
        self.config.provider_type.base_url().trim_end_matches('/')
    }

    async fn get_premium_verification(&self, access_token: &str) -> Result<PremiumVerificationStatus> {
        let response = reqwest::Client::new()
            .get(format!("{}/api/premium-verification", self.base_url()))
            .bearer_auth(access_token)
            .send()
            .await
            .context("Failed to request LittleSkin premium verification")?;

        match response.status() {
            StatusCode::OK => {
                let data: PremiumVerificationVerified = response
                    .json()
                    .await
                    .context("Failed to parse LittleSkin premium verification response")?;
                Ok(PremiumVerificationStatus {
                    verified: data.verified,
                    uuid: Some(data.uuid),
                })
            }
            StatusCode::NOT_FOUND => {
                let data: PremiumVerificationNotVerified = response
                    .json()
                    .await
                    .context("Failed to parse LittleSkin premium verification 404 response")?;
                Ok(PremiumVerificationStatus {
                    verified: data.verified,
                    uuid: None,
                })
            }
            status => Err(Error::msg(format!(
                "LittleSkin premium verification request failed with status {status}"
            ))),
        }
    }
}

#[async_trait]
impl OAuthProvider for LittleSkinProvider {
    fn get_authorize_url(&self, redirect_uri: &str, state: &str) -> String {
        let scopes = &self.config.scopes;
        format!(
            "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}&scope={}",
            self.base_url(),
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(redirect_uri),
            state,
            urlencoding::encode(&scopes.join(" "))
        )
    }

    async fn exchange_token(&self, code: &str, redirect_uri: &str) -> Result<(String, Duration)> {
        let token_data: HashMap<String, serde_json::Value> = reqwest::Client::new()
            .post(format!("{}/oauth/token", self.base_url()))
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("redirect_uri", redirect_uri),
                ("code", code),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let access_token = token_data.get("access_token")
            .ok_or_else(|| Error::msg("access_token field does not exist"))?
            .as_str()
            .ok_or_else(|| Error::msg("access_token is not a string"))?
            .to_string();
        let expires_in = token_data.get("expires_in")
            .ok_or_else(|| Error::msg("expires_in field does not exist"))?
            .as_u64()
            .ok_or_else(|| Error::msg("expires_in field is not a positive integer"))?;

        Ok((access_token, Duration::from_secs(expires_in)))
    }

    async fn get_user_info(&self, access_token: &str) -> Result<UnifiedUserInfo> {
        let client = reqwest::Client::new();

        let user_info: LittleSkinUserInfo = client
            .get(format!("{}/api/user", self.base_url()))
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        debug!("LittleSkin user info fetched successfully: nickname={}", user_info.nickname);

        let players: Vec<LittleSkinPlayer> = client
            .get(format!("{}/api/players", self.base_url()))
            .bearer_auth(access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let names = players.into_iter().map(|player| player.name).collect::<Vec<String>>();
        let mut profiles: Vec<YggdrasilProfile> = client
            .post(format!("{}/api/yggdrasil/api/profiles/minecraft", self.base_url()))
            .json(&names)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let premium_verification = self.get_premium_verification(access_token).await?;

        if let Some(premium_uuid) = premium_verification.uuid {
            if profiles.iter().all(|profile| profile.id != premium_uuid) {
                match get_profile_from_uuid(&premium_uuid).await {
                    Ok(profile) => profiles.push(profile),
                    Err(error) => warn!(
                        ?error,
                        premium_uuid = %premium_uuid,
                        "Failed to fetch Mojang profile for LittleSkin premium verification UUID"
                    ),
                }
            }
        }

        Ok(UnifiedUserInfo {
            nickname: user_info.nickname,
            provider: self.name.clone(),
            provider_type: OAuthProviderType::LittleSkin,
            premium_verification: Some(premium_verification),
            profiles,
        })
    }

    fn provider_type(&self) -> OAuthProviderType {
        OAuthProviderType::LittleSkin
    }
}