use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router
};
use tower_http::trace::{self, TraceLayer};
use std::net::{SocketAddr, Ipv6Addr};
use std::sync::Arc;
use tracing::{Level, error, info, warn};
use tracing_subscriber;

mod app;
mod static_content;
mod external_api;
mod oauth;
mod rcon;
mod config;
mod ysm;

use app::{AppState, AppResult};

const YSM_UPLOAD_MAX_BODY_SIZE: usize = 64 * 1024 * 1024;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing logs
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_level(true)
        .init();

    
    let app_state = Arc::new(AppState::new());

    // Routes that require authentication
    let protected_routes = Router::new()
        .route("/api/user", get(oauth::get_user))
        .route(
            "/api/ysm/upload",
            post(ysm::upload_authorized_model).layer(DefaultBodyLimit::max(YSM_UPLOAD_MAX_BODY_SIZE)),
        )
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            oauth::auth_middleware
        ));
    
    // Create routes
    let app = Router::new()
        // OAuth2 provider list
        .route("/api/oauth/providers", get(oauth::list_providers))
        // Dynamic OAuth2 routes supporting multiple providers
        .route("/api/oauth/{provider}/login", get(oauth::login))
        .route("/api/oauth/{provider}/callback", get(oauth::callback))
        // Logout
        .route("/api/logout", get(oauth::logout))
        // Merge authenticated routes
        .merge(protected_routes)
        .with_state(app_state.clone())
        // API request tracing
        .layer(TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
            .on_request(trace::DefaultOnRequest::new().level(Level::DEBUG))
            .on_response(trace::DefaultOnResponse::new().level(Level::INFO))
        )
        // Static file serving
        .fallback(static_content::serve_static);
    
    // If you need to protect other APIs with authentication, you can do it like this:
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

    // Bind the address
    let addr = SocketAddr::from((
        app_state.config.server.host.parse::<std::net::IpAddr>()?,
        app_state.config.server.port
    ));
    
    info!("Server starting at: http://{}", addr);
    info!("OAuth callback base URL: {}/api/oauth/[provider]/callback", app_state.config.oauth.prefix_url);
    
    // Show all enabled providers
    let enabled_providers = app_state.get_enabled_providers();
    if enabled_providers.is_empty() {
        warn!("No OAuth providers are enabled!");
    } else {
        info!("Enabled OAuth providers:");
        for (name, provider) in enabled_providers {
            info!("  - {} ({}): {}/api/oauth/{}/login", 
                name,
                provider.provider_type.display_name(),
                app_state.config.oauth.prefix_url,
                name
            );
        }
    }

    // Start the server
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind address {}: {:?}", addr, e);
            std::process::exit(1);
        }
    };
    
    info!("Server is running...");
    
    axum::serve(listener, app).await
        .inspect_err(|e| error!("Error: {e:?}"))?;

    Ok(())
}
