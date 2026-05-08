pub mod blessingskin;
pub mod littleskin;
pub mod microsoft;

use crate::{AppResult, external_api::YggdrasilProfile};

use axum::{
    extract::{Path, Query, State, FromRequestParts, Request},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Redirect, Response},
    Json,
    middleware::Next,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use jwt::{SignWithKey, VerifyWithKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::{fmt, str::FromStr, sync::Arc, time::{Duration, SystemTime}};
use crate::AppState;
use tracing::{info, debug};

use anyhow::Result;

use async_trait::async_trait;

// ============= Common Data Structures =============

/// OAuth2 authorization code query parameters
#[derive(Serialize, Deserialize)]
pub struct AuthRequest {
    pub code: String,
    pub state: String,
}


/// Unified user information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedUserInfo {
    pub nickname: String,
    pub provider: String,       // Provider name
    pub provider_type: OAuthProviderType,  // Provider type
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub premium_verification: Option<PremiumVerificationStatus>,
    #[serde(default)]
    pub profiles: Vec<YggdrasilProfile>,  // Player profile list
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PremiumVerificationStatus {
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenInformation {
    pub access_token: String,
    pub provider_name: String,
    pub expire_date: SystemTime,
    pub user_info: UnifiedUserInfo
}

impl<S> FromRequestParts<S> for UnifiedUserInfo
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract user info from extensions, inserted by auth_middleware
        parts
            .extensions
            .get::<UnifiedUserInfo>()
            .cloned()
            .ok_or_else(|| {
                (StatusCode::UNAUTHORIZED, "Authentication required").into_response()
            })
    }
}

// ============= OAuth Provider Trait =============

/// OAuth provider type enum
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OAuthProviderType {
    /// Blessing Skin site
    BlessingSkin(String),
    /// LittleSkin
    LittleSkin,
    /// Microsoft account
    Microsoft,
}

impl OAuthProviderType {

    /// Get the provider display name
    pub fn display_name(&self) -> String {
        match self {
            Self::BlessingSkin(prefix) => format!("Blessing Skin ({prefix})"),
            Self::LittleSkin => "LittleSkin".to_string(),
            Self::Microsoft => "Microsoft".to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        match self {
            Self::BlessingSkin(url) => url,
            Self::LittleSkin => "https://littleskin.cn",
            Self::Microsoft => "https://login.microsoftonline.com",
        }
    }
}

impl fmt::Display for OAuthProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlessingSkin(prefix) => write!(f, "blessingskin={}", prefix),
            Self::LittleSkin => write!(f, "littleskin"),
            Self::Microsoft => write!(f, "microsoft"),
        }
    }
}

impl FromStr for OAuthProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("bs=") || s.starts_with("blessingskin=") || s.starts_with("blessing-skin=") {
            let split_index = s.find('=')
                .expect("Equal sign should appear");
            let prefix = &s[split_index+1..];
            return Ok(Self::BlessingSkin(prefix.to_string()))
        }
        match s.to_lowercase().as_str() {
            "littleskin" | "ls" => Ok(Self::LittleSkin),
            "microsoft" | "ms" => Ok(Self::Microsoft),
            _ => Err(format!("Unknown provider type: {}", s)),
        }
    }
}

impl Serialize for OAuthProviderType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'a> Deserialize<'a> for OAuthProviderType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let s = String::deserialize(deserializer)?;
        OAuthProviderType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// OAuth provider interface
/// 
/// Every OAuth provider must implement this trait
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// Get the authorization URL
    fn get_authorize_url(&self, redirect_uri: &str, state: &str) -> String;
    
    /// Exchange an authorization code for an access token
    async fn exchange_token(&self, code: &str, redirect_uri: &str) -> Result<(String, Duration)>;
    
    /// Fetch user information
    async fn get_user_info(&self, access_token: &str) -> Result<UnifiedUserInfo>;
    
    /// Get the provider type
    fn provider_type(&self) -> OAuthProviderType;
}

/// Create an OAuth provider instance from configuration
/// 
/// # Parameters
/// 
/// * `provider_config` - OAuth provider configuration
/// * `provider_name` - Provider name
/// 
/// # Returns
/// 
/// An OAuthProvider trait object of the matching type
pub fn create_oauth_provider(
    provider_config: &crate::config::OAuthProviderConfig,
    provider_name: &str,
) -> Box<dyn OAuthProvider> {
    match provider_config.provider_type {
        OAuthProviderType::LittleSkin => Box::new(
            littleskin::LittleSkinProvider::new(provider_config.clone(), provider_name.to_string())
        ),
        OAuthProviderType::BlessingSkin(ref url) if is_littleskin_url(url) => Box::new(
            littleskin::LittleSkinProvider::new(provider_config.clone(), provider_name.to_string())
        ),
        OAuthProviderType::BlessingSkin(_) => Box::new(
            blessingskin::BlessingSkinProvider::new(provider_config.clone(), provider_name.to_string())
        ),
        OAuthProviderType::Microsoft => Box::new(
            microsoft::MicrosoftProvider::new(provider_config.clone(), provider_name.to_string())
        )
    }
}

fn is_littleskin_url(url: &str) -> bool {
    let normalized = url.trim_end_matches('/').to_ascii_lowercase();
    normalized == "https://littleskin.cn" || normalized == "http://littleskin.cn"
}

// ============= Route Handlers =============

/// List all available OAuth providers
pub async fn list_providers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let providers: Vec<_> = state
        .get_enabled_providers()
        .into_iter()
        .map(|(name, provider_config)| {
            serde_json::json!({
                "name": name,
                "provider_type": provider_config.provider_type,
                "display_name": provider_config.provider_type.display_name(),
                "login_url": format!("/api/oauth/{}/login", name)
            })
        })
        .collect();

    Json(serde_json::json!({
        "providers": providers
    }))
}


/// Start the OAuth2 login flow for a dynamic route
pub async fn login(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
) -> AppResult<Response> {
    info!("Starting {} OAuth2 login flow", provider_name);

    // Get the provider configuration
    let provider_config = match state.get_provider(&provider_name) {
        Some(config) => config,
        None => {
            return Ok((StatusCode::NOT_FOUND, format!("Provider {} not found", provider_name)).into_response());
        }
    };
    
    if !provider_config.enabled {
        return Ok((StatusCode::FORBIDDEN, format!("Provider {} is disabled", provider_name)).into_response());
    }
    
    let redirect_uri = state.get_redirect_uri(&provider_name);
    
    debug!("redirect_uri: {}", redirect_uri);
    
    // Create the matching provider implementation
    let provider = create_oauth_provider(provider_config, &provider_name);
    
    let state_token = Uuid::new_v4().sign_with_key(state.secret())?;
    let auth_url = provider.get_authorize_url(&redirect_uri, &state_token);
    
    Ok(Redirect::to(&auth_url).into_response())
}

/// Handle the OAuth2 callback for a dynamic route
pub async fn callback(
    jar: CookieJar,
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    Query(params): Query<AuthRequest>,
) -> AppResult<Response> {
    debug!("Received {} OAuth2 callback", provider_name);
    debug!("Authorization code: {}", params.code);
    debug!("Authorization state: {}", params.state);

    let action_uuid: Uuid = match params.state.verify_with_key(state.secret()) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Ok((StatusCode::UNAUTHORIZED, jar, "State verification failed".to_string()).into_response());
        }
    };
    debug!("Authorization UUID: {}", action_uuid.to_string());

    // Get the provider configuration
    let provider_config = match state.get_provider(&provider_name) {
        Some(config) => config,
        None => {
            return Ok((StatusCode::NOT_FOUND, jar, format!("Provider {} not found", provider_name)).into_response());
        }
    };
    
    let redirect_uri = state.get_redirect_uri(&provider_name);
    
    // Create the matching provider implementation
    let provider = create_oauth_provider(provider_config, &provider_name);
    
    // 1. Exchange the authorization code for an access token
    let (access_token, expire_duration) = provider.exchange_token(&params.code, &redirect_uri).await?;

    debug!("Get a access token expiring in {}s", expire_duration.as_secs());
    
    // 2. Fetch user information
    let user_info = provider.get_user_info(&access_token).await?;
    
    debug!("User info fetched successfully: nickname={}", user_info.nickname);
    
    // 3. Create the token and set the cookie
    let token = TokenInformation {
        access_token,
        provider_name,
        user_info,
        expire_date: SystemTime::now() + expire_duration
    }
    .sign_with_key(state.secret())?;
    
    let mut token_cookie = Cookie::new("access_token", token);
    token_cookie.set_path("/");
    token_cookie.set_http_only(true);
    token_cookie.set_same_site(SameSite::Strict);
    token_cookie.set_expires(time::OffsetDateTime::now_utc() + expire_duration);
    
    let jar = jar.add(token_cookie);
    
    // Redirect to the home page
    Ok((jar, Redirect::to("/")).into_response())
}



/// Get the current user information
/// 
/// This function depends on auth_middleware inserting user info into request extensions
pub async fn get_user(user: UnifiedUserInfo) -> Json<UnifiedUserInfo> {
    Json(user)
}

/// Log out
pub async fn logout(jar: CookieJar) -> impl IntoResponse {
    info!("User logged out");
    
    let mut token_cookie = Cookie::from("access_token");
    token_cookie.set_path("/");
    
    let jar = jar.remove(token_cookie);
    
    (jar, Redirect::to("/"))
}

/// Authentication middleware
/// 
/// This middleware validates the user's authentication state, fetches user info
/// from the OAuth server, and stores it in request extensions for downstream handlers.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, CookieJar, String)> {
    // Read the token from the cookie
    let token_cookie = match jar.get("access_token") {
        Some(x) => x,
        None => { return Err((StatusCode::UNAUTHORIZED, jar, "Not authenticated".to_string())); }
    };

    // Validate and parse the token
    let token_claims: TokenInformation = match token_cookie.value().verify_with_key(state.secret()) {
        Ok(x) => x,
        Err(_) => {
            return Err((StatusCode::UNAUTHORIZED, jar.remove(Cookie::from("access_token")), "Invalid token".to_string()));
        }
    };

    // Check whether the token has expired
    if SystemTime::now() > token_claims.expire_date {
        return Err((StatusCode::UNAUTHORIZED, jar.remove(Cookie::from("access_token")), "Login token expired".to_string()));
    }

    // Retrieve user info from the OAuth server
    let user_info = token_claims.user_info;

    debug!("User authorized: {user_info:?}");

    // Store user info in request extensions
    request.extensions_mut().insert(user_info);

    // Continue handling the request
    Ok(next.run(request).await)
}
