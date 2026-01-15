pub mod blessingskin;

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
use async_trait::async_trait;

use anyhow::Result;

// ============= 通用数据结构 =============

/// OAuth2 授权码查询参数
#[derive(Deserialize)]
pub struct AuthRequest {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct YggdrasilKVPair {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct YggdrasilProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub properties: Vec<YggdrasilKVPair>,
}

/// 统一的用户信息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedUserInfo {
    pub uid: String,           // 统一使用字符串 ID
    pub nickname: String,
    pub email: String,
    pub provider: String,       // 提供者名称
    pub provider_type: OAuthProviderType,  // 提供者类型
    #[serde(default)]
    pub profiles: Vec<YggdrasilProfile>,  // 玩家角色列表
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
        // 从 extensions 中提取用户信息（由 auth_middleware 插入）
        parts
            .extensions
            .get::<UnifiedUserInfo>()
            .cloned()
            .ok_or_else(|| {
                (StatusCode::UNAUTHORIZED, "Authentication required").into_response()
            })
    }
}

// ============= OAuth 提供者 Trait =============

/// OAuth 提供者类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OAuthProviderType {
    /// Blessing Skin 皮肤站
    BlessingSkin(String),
    /// Microsoft 账号
    Microsoft,
}

impl OAuthProviderType {

    /// 获取提供者的显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::BlessingSkin(prefix) => format!("Blessing Skin ({prefix})"),
            Self::Microsoft => "Microsoft".to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        match self {
            Self::BlessingSkin(url) => url,
            Self::Microsoft => "https://login.microsoftonline.com",
        }
    }
}

impl fmt::Display for OAuthProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlessingSkin(prefix) => write!(f, "blessingskin={}", prefix),
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

/// OAuth 提供者接口
/// 
/// 每个 OAuth 提供者都需要实现这个 trait
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// 获取授权 URL
    fn get_authorize_url(&self, redirect_uri: &str, state: &str) -> String;
    
    /// 使用授权码交换访问令牌
    async fn exchange_token(&self, code: &str, redirect_uri: &str) -> Result<(String, Duration)>;
    
    /// 获取用户信息
    async fn get_user_info(&self, access_token: &str) -> Result<UnifiedUserInfo>;
    
    /// 获取提供者类型
    fn provider_type(&self) -> OAuthProviderType;
}

/// 根据配置创建 OAuth 提供者实例
/// 
/// # 参数
/// 
/// * `provider_config` - OAuth 提供者配置
/// * `provider_name` - 提供者名称
/// 
/// # 返回
/// 
/// 返回对应类型的 OAuthProvider trait 对象
pub fn create_oauth_provider(
    provider_config: &crate::config::OAuthProviderConfig,
    provider_name: &str,
) -> Box<dyn OAuthProvider> {
    match provider_config.provider_type {
        OAuthProviderType::BlessingSkin(_) => Box::new(
            blessingskin::BlessingSkinProvider::new(provider_config.clone(), provider_name.to_string())
        ),
        OAuthProviderType::Microsoft => todo!()
    }
}

// ============= 路由处理函数 =============

/// 列出所有可用的 OAuth 提供者
pub async fn list_providers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let providers: Vec<_> = state
        .get_enabled_providers()
        .into_iter()
        .map(|(name, provider_config)| {
            serde_json::json!({
                "name": name,
                "type": provider_config.provider_type,
                "display_name": provider_config.provider_type.display_name(),
                "login_url": format!("/api/oauth/{}/login", name)
            })
        })
        .collect();

    Json(serde_json::json!({
        "providers": providers
    }))
}


/// 开始 OAuth2 登录流程（动态路由）
pub async fn login(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("启动 {} OAuth2 登录流程", provider_name);

    // 获取提供者配置
    let provider_config = state
        .get_provider(&provider_name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Provider {} not found", provider_name)))?;
    
    if !provider_config.enabled {
        return Err((StatusCode::FORBIDDEN, format!("Provider {} is disabled", provider_name)));
    }
    
    let redirect_uri = state.get_redirect_uri(&provider_name);
    
    debug!("redirect_uri: {}", redirect_uri);
    
    // 根据提供者类型创建相应的 provider
    let provider = create_oauth_provider(provider_config, &provider_name);
    
    let state_token = Uuid::new_v4().sign_with_key(state.secret())
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Server failed to sign JWT".to_string()))?;
    let auth_url = provider.get_authorize_url(&redirect_uri, &state_token);
    
    Ok(Redirect::to(&auth_url))
}

/// OAuth2 回调处理（动态路由）
pub async fn callback(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    Query(params): Query<AuthRequest>,
    jar: CookieJar,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    debug!("Received {} OAuth2 callback", provider_name);
    debug!("Authorization code: {}", params.code);
    debug!("Authorization state: {}", params.state);

    let action_uuid: Uuid = params.state.verify_with_key(state.secret())
        .map_err(|_| (StatusCode::UNAUTHORIZED, "State verification failed".to_string()))?;
    debug!("Authorization UUID: {}", action_uuid.to_string());

    // 获取提供者配置
    let provider_config = state
        .get_provider(&provider_name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Provider {} not found", provider_name)))?;
    
    let redirect_uri = state.get_redirect_uri(&provider_name);
    
    // 根据提供者类型创建相应的 provider
    let provider = create_oauth_provider(provider_config, &provider_name);
    
    // 1. 使用授权码交换访问令牌
    let (access_token, expire_duration) = provider.exchange_token(&params.code, &redirect_uri).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    debug!("Get a access token expiring in {}s", expire_duration.as_secs());
    
    // 2. 获取用户信息
    let user_info = provider.get_user_info(&access_token).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    debug!("用户信息获取成功: uid={}, nickname={}", user_info.uid, user_info.nickname);
    
    // 3. 创建 token 并设置 cookie
    let token = TokenInformation {
        access_token,
        provider_name,
        user_info,
        expire_date: SystemTime::now() + expire_duration
    }
    .sign_with_key(state.secret())
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Token sign failed: {}", e)))?;
    
    let mut token_cookie = Cookie::new("access_token", token);
    token_cookie.set_path("/");
    token_cookie.set_http_only(true);
    token_cookie.set_same_site(SameSite::Strict);
    token_cookie.set_expires(time::OffsetDateTime::now_utc() + expire_duration);
    
    let jar = jar.add(token_cookie);
    
    // 重定向到首页
    Ok((jar, Redirect::to("/")))
}



/// 获取当前用户信息
/// 
/// 此函数依赖于 auth_middleware 将用户信息注入到请求的 extensions 中
pub async fn get_user(user: UnifiedUserInfo) -> Json<UnifiedUserInfo> {
    debug!("返回用户信息: uid={}, nickname={}", user.uid, user.nickname);
    Json(user)
}

/// 登出
pub async fn logout(jar: CookieJar) -> impl IntoResponse {
    info!("用户登出");
    
    let mut token_cookie = Cookie::from("access_token");
    token_cookie.set_path("/");
    
    let jar = jar.remove(token_cookie);
    
    (jar, Redirect::to("/"))
}

/// 认证中间件
/// 
/// 此中间件会验证用户的认证状态，并从 OAuth 服务器获取用户信息，
/// 然后将用户信息存储到请求的 extensions 中，供下游 handler 使用。
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, CookieJar, String)> {
    // 从 cookie 中获取 token
    let token_cookie = match jar.get("access_token") {
        Some(x) => x,
        None => { return Err((StatusCode::UNAUTHORIZED, jar, "Not authenticated".to_string())); }
    };

    // 验证并解析 token
    let token_claims: TokenInformation = match token_cookie.value().verify_with_key(state.secret()) {
        Ok(x) => x,
        Err(_) => {
            return Err((StatusCode::UNAUTHORIZED, jar.remove(Cookie::from("access_token")), "Invalid token".to_string()));
        }
    };

    // 检查 token 是否过期
    if SystemTime::now() > token_claims.expire_date {
        return Err((StatusCode::UNAUTHORIZED, jar.remove(Cookie::from("access_token")), "Login token expired".to_string()));
    }

    // 从 OAuth 服务器获取用户信息
    let user_info = token_claims.user_info;

    debug!("User authorized: {user_info:?}");

    // 将用户信息存储到请求的 extensions 中
    request.extensions_mut().insert(user_info);

    // 继续处理请求
    Ok(next.run(request).await)
}
