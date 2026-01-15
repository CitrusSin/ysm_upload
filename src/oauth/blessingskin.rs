use std::time::Duration;

use super::{OAuthProvider, OAuthProviderType, UnifiedUserInfo};
use crate::{config::OAuthProviderConfig, oauth::YggdrasilProfile};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use anyhow::Result;


#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BlessingSkinUserInfo {
    pub uid: u64,
    pub nickname: String,
    pub email: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BlessingSkinProfile {
    pub pid: i32,
    pub uid: i32,
    pub name: String,
    pub tid_skin: i32,
    pub tid_cape: i32,
    pub last_modified: String,
}

pub struct BlessingSkinProvider {
    config: OAuthProviderConfig,
    name: String,
}

impl BlessingSkinProvider {
    pub fn new(config: OAuthProviderConfig, name: String) -> Self {
        Self { config, name }
    }
}

#[async_trait]
impl OAuthProvider for BlessingSkinProvider {
    fn get_authorize_url(&self, redirect_uri: &str, state: &str) -> String {
        let scopes = &self.config.scopes;
        
        // 从 provider_type 中提取 base URL
        let base_url = self.config.provider_type.base_url().trim_end_matches('/');
        
        format!(
            "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}&scope={}",
            base_url,
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(redirect_uri),
            state,
            scopes.join(" ")
        )
    }

    async fn exchange_token(&self, code: &str, redirect_uri: &str) -> Result<(String, Duration)> {
        let client = reqwest::Client::new();
        
        // 从 provider_type 中提取 base URL
        let base_url = self.config.provider_type.base_url().trim_end_matches('/');
        
        let token_data: TokenResponse = client
            .post(format!("{}/oauth/token", base_url))
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("redirect_uri", redirect_uri),
                ("code", code),
            ])
            .send().await?.error_for_status()?
            .json().await?;

        debug!("Token 获取成功");
        Ok((token_data.access_token, Duration::from_secs(token_data.expires_in)))
    }

    async fn get_user_info(&self, access_token: &str) -> Result<UnifiedUserInfo> {
        let client = reqwest::Client::new();
        
        // 从 provider_type 中提取 base URL
        let base_url = match &self.config.provider_type {
            OAuthProviderType::BlessingSkin(url) => url.as_str(),
            _ => panic!("Invalid provider type for BlessingSkinProvider"),
        };
        
        let user_info: BlessingSkinUserInfo = client
            .get(format!("{}/api/user", base_url))
            .bearer_auth(access_token)
            .send().await?
            .json().await?;

        debug!("BlessingSkin 用户信息获取成功: uid={}, nickname={}", user_info.uid, user_info.nickname);

        // 获取profiles
        let profs: Vec<BlessingSkinProfile> = client
            .get(format!("{}/api/players", base_url))
            .bearer_auth(access_token)
            .send().await?
            .json().await?;

        let names = profs.into_iter().map(|p| p.name).collect::<Vec<String>>();

        debug!("Player names: {:?}", names);

        let profiles: Vec<YggdrasilProfile> = client
            .post(format!("{}/api/yggdrasil/api/profiles/minecraft", base_url))
            .json(&names)
            .send().await?
            .json().await?;

        debug!("Profiles: {:?}", profiles);

        // 转换为统一格式
        Ok(UnifiedUserInfo {
            uid: user_info.uid.to_string(),
            nickname: user_info.nickname,
            email: user_info.email,
            provider: self.name.clone(),
            provider_type: self.provider_type(),
            profiles,
        })
    }

    fn provider_type(&self) -> OAuthProviderType {
        self.config.provider_type.clone()
    }
}
