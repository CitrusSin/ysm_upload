use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router
};
use clap::Parser;
use tower_http::trace::{self, TraceLayer};
use std::{net::SocketAddr, path::Path};
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
mod storage;

use app::{AppState, AppResult};

const YSM_UPLOAD_MAX_BODY_SIZE: usize = 64 * 1024 * 1024;

#[derive(Parser)]
struct CliArgs {
    /// Path to the configuration file.
    #[clap(short, long, default_value = "config.yml")]
    config: String,

    /// Enable debug logging.
    /// This will print more detailed logs, including request and response bodies, which can be useful for troubleshooting.
    #[clap(short = 'v', long)]
    verbose: bool,

    /// Show the version information.
    /// This will print the version of the application and exit.
    #[clap(short = 'V', long)]
    version: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli_args = CliArgs::parse();

    if cli_args.version {
        println!("YSM Upload Server version {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    
    // Initialize tracing logs
    tracing_subscriber::fmt()
        .with_max_level(if cli_args.verbose { Level::DEBUG } else { Level::INFO })
        .with_target(false)
        .with_level(true)
        .init();

    let app_state = Arc::new(AppState::new(Path::new(&cli_args.config)));

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
    let app_routes = Router::new()
        // OAuth2 provider list
        .route("/api/oauth/providers", get(oauth::list_providers))
        // Dynamic OAuth2 routes supporting multiple providers
        .route("/api/oauth/{provider}/login", get(oauth::login))
        .route("/api/oauth/{provider}/callback", get(oauth::callback))
        // Logout
        .route("/api/logout", get(oauth::logout))
        // Merge authenticated routes
        .merge(protected_routes)
        // API request tracing
        .layer(TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
            .on_request(trace::DefaultOnRequest::new().level(Level::DEBUG))
            .on_response(trace::DefaultOnResponse::new().level(Level::INFO))
        )
        // Static file serving
        .fallback(static_content::serve_static);

    let app = if app_state.frontend_base_path() == "/" {
        app_routes
    } else {
        Router::new()
            .route(app_state.frontend_base_path(), get(static_content::redirect_to_base))
            .route(&app_state.frontend_base_href(), get(static_content::serve_static))
            .nest(app_state.frontend_base_path(), app_routes)
    }.with_state(app_state.clone());
    
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
    info!("OAuth callback base URL: {}/api/oauth/[provider]/callback", app_state.public_base_url());
    info!("Frontend base path: {}", app_state.frontend_base_path());
    
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
                app_state.public_base_url(),
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
