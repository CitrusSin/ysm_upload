use axum::{routing::get, Router};
use hmac::Hmac;
use hmac::digest::KeyInit;
use sha2::Sha256;
use tower_http::trace::{self, TraceLayer};
use std::{net::SocketAddr, path::Path};
use std::sync::Arc;
use tracing::{Level, error, info, warn};
use tracing_subscriber;
use anyhow::Result;

use crate::config::{Config, OAuthProviderConfig};

mod static_content;
mod oauth;
mod config;

const CONFIG_FILE: &str = "config.yml";

pub struct AppState {
    pub config: Config,
    
    secret_key: Hmac<Sha256>
}

impl AppState {
    pub fn new() -> Self {
        // 检查配置文件是否存在
        if !Path::new(CONFIG_FILE).exists() {
            warn!("配置文件不存在，正在创建默认配置文件...");
            
            match config::Config::create_default(CONFIG_FILE) {
                Ok(_) => {
                    info!("已创建默认配置文件: {}", CONFIG_FILE);
                    info!("请修改配置文件后重新运行程序");
                    std::process::exit(0);
                }
                Err(e) => {
                    error!("创建配置文件失败: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        // 加载配置文件
        let app_config = match config::Config::load(CONFIG_FILE) {
            Ok(config) => {
                info!("配置文件加载成功: {}", CONFIG_FILE);
                config
            }
            Err(e) => {
                error!("配置文件加载失败: {:?}", e);
                std::process::exit(1);
            }
        };

        let secret_key = Hmac::<Sha256>::new_from_slice(app_config.oauth.secret_string.as_bytes())
            .expect("HMAC can take key of any size");

        AppState { config: app_config, secret_key }
    }


    /// 获取重定向 URL
    pub fn get_redirect_uri(&self, provider: &str) -> String {
        format!("{}/api/oauth/{}/callback", self.config.oauth.prefix_url, provider)
    }

    /// 获取所有启用的提供者
    pub fn get_enabled_providers(&self) -> Vec<(String, &OAuthProviderConfig)> {
        self.config.oauth.providers
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, config)| (name.clone(), config))
            .collect()
    }

    /// 获取特定提供者配置
    pub fn get_provider(&self, name: &str) -> Option<&OAuthProviderConfig> {
        self.config.oauth.providers.get(name)
    }

    pub fn secret(&self) -> &Hmac<Sha256> {
        &self.secret_key
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化 tracing 日志
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .with_level(true)
        .init();
    
    let app_state = Arc::new(AppState::new());

    // 需要认证的路由
    let protected_routes = Router::new()
        .route("/api/user", get(oauth::get_user))
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            oauth::auth_middleware
        ));
    
    // 创建路由
    let app = Router::new()
        // OAuth2 提供者列表
        .route("/api/oauth/providers", get(oauth::list_providers))
        // OAuth2 动态路由（支持多个提供者）
        .route("/api/oauth/{provider}/login", get(oauth::login))
        .route("/api/oauth/{provider}/callback", get(oauth::callback))
        // 登出
        .route("/api/logout", get(oauth::logout))
        // 合并需要认证的路由
        .merge(protected_routes)
        .with_state(app_state.clone())
        // API 请求跟踪
        .layer(TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
            .on_request(trace::DefaultOnRequest::new().level(Level::DEBUG))
            .on_response(trace::DefaultOnResponse::new().level(Level::INFO))
        )
        // 静态文件服务
        .fallback(static_content::serve_static);
    
    // 如果需要为其他 API 添加认证保护，可以这样做：
    // let protected_routes = Router::new()
    //     .route("/api/upload", post(your_upload_handler))
    //     .route("/api/profile", get(your_profile_handler))
    //     .layer(axum::middleware::from_fn_with_state(
    //         app_state.clone(),
    //         oauth::auth_middleware
    //     ));
    // 
    // let app = Router::new()
    //     .route("/api/oauth/providers", get(oauth::list_providers))
    //     .route("/api/oauth/{provider}/login", get(oauth::login))
    //     .route("/api/oauth/{provider}/callback", get(oauth::callback))
    //     .route("/api/logout", get(oauth::logout))
    //     .merge(protected_routes)
    //     .with_state(app_state.clone())
    //     .fallback(static_content::serve_static);

    // 绑定地址
    let addr = SocketAddr::from((
        app_state.config.server.host.parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        app_state.config.server.port
    ));
    
    info!("服务器启动地址: http://{}", addr);
    info!("OAuth 回调基础地址: {}/api/oauth/[provider]/callback", app_state.config.oauth.prefix_url);
    
    // 显示所有启用的提供者
    let enabled_providers = app_state.get_enabled_providers();
    if enabled_providers.is_empty() {
        warn!("没有启用任何 OAuth 提供者!");
    } else {
        info!("启用的 OAuth 提供者:");
        for (name, provider) in enabled_providers {
            info!("  - {} ({}): {}/api/oauth/{}/login", 
                name,
                provider.provider_type.display_name(),
                app_state.config.oauth.prefix_url,
                name
            );
        }
    }

    // 启动服务器
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("绑定地址失败 {}: {:?}", addr, e);
            std::process::exit(1);
        }
    };
    
    info!("服务器正在运行...");
    
    axum::serve(listener, app).await
        .inspect_err(|e| error!("Error: {e:?}"))?;

    Ok(())
}
