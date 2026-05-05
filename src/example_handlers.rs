// Example: how to use the OAuth middleware in a new module
// This file shows how to create APIs that require user authentication

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::oauth::AuthUser;

// ============= Example 1: Simple User Profile API =============

/// Get the current user's profile
/// 
/// Automatically retrieve authenticated user info with the AuthUser parameter
pub async fn get_profile(user: AuthUser) -> impl IntoResponse {
    Json(json!({
        "uid": user.uid,
        "nickname": user.nickname,
        "email": user.email,
        "players": user.players
    }))
}

// ============= Example 2: File Upload API =============

/// Handle file uploads
/// 
/// Automatically retrieve user info and associate uploaded files with the user
pub async fn upload_file(
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut uploaded_files = Vec::new();
    
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap_or("unknown").to_string();
        let filename = field.file_name().unwrap_or("unnamed").to_string();
        let data = field.bytes().await.unwrap();
        
        // Add your file persistence logic here
        // save_file_to_storage(user.uid, &filename, &data).await?;
        
        tracing::info!(
            "User {} (UID: {}) uploaded file: {} ({} bytes)",
            user.nickname,
            user.uid,
            filename,
            data.len()
        );
        
        uploaded_files.push(json!({
            "name": filename,
            "size": data.len()
        }));
    }
    
    Ok(Json(json!({
        "success": true,
        "message": format!("Files uploaded by {}", user.nickname),
        "files": uploaded_files
    })))
}

// ============= Example 3: API With Query Parameters =============

#[derive(Deserialize)]
pub struct FileListQuery {
    page: Option<u32>,
    limit: Option<u32>,
}

/// List a user's files
/// 
/// Combine Query and AuthUser parameters
pub async fn list_user_files(
    user: AuthUser,
    Query(query): Query<FileListQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(10);
    
    // Add your database query logic here
    // let files = db.get_user_files(user.uid, page, limit).await;
    
    Json(json!({
        "user": {
            "uid": user.uid,
            "nickname": user.nickname
        },
        "pagination": {
            "page": page,
            "limit": limit
        },
        "files": [] // File list queried from the database
    }))
}

// ============= Example 4: API With Path Parameters =============

/// Get a specific file
/// 
/// Includes an authorization check: only the file owner can access it
pub async fn get_file(
    user: AuthUser,
    Path(file_id): Path<u64>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Add your database query logic here
    // let file = db.get_file(file_id).await
    //     .ok_or_else(|| (StatusCode::NOT_FOUND, "File not found".to_string()))?;
    
    // Example authorization check
    // if file.owner_uid != user.uid {
    //     return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    // }
    
    Ok(Json(json!({
        "file_id": file_id,
        "owner": {
            "uid": user.uid,
            "nickname": user.nickname
        },
        "message": "File details would be returned here"
    })))
}

// ============= Example 5: API With State =============

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: String, // This should be a real database connection pool
}

/// Update user settings
/// 
/// Combine State and AuthUser
pub async fn update_settings(
    user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    tracing::info!(
        "User {} (UID: {}) is updating settings",
        user.nickname,
        user.uid
    );
    
    // Add your database update logic here
    // db.update_user_settings(user.uid, &payload).await?;
    
    Ok(Json(json!({
        "success": true,
        "message": "Settings updated successfully",
        "user": user.nickname
    })))
}

// ============= Example 6: Optional Authentication =============

/// Get public content
/// 
/// Support optional authentication: guests can access this too, but logged-in users see personalized content
pub async fn get_public_content(
    user: Option<AuthUser>, // Use Option to make authentication optional
) -> impl IntoResponse {
    match user {
        Some(user) => Json(json!({
            "authenticated": true,
            "message": format!("Welcome back, {}!", user.nickname),
            "personalized_content": "Your personalized feed..."
        })),
        None => Json(json!({
            "authenticated": false,
            "message": "Welcome, guest!",
            "public_content": "Public feed..."
        }))
    }
}

// ============= How To Use These Handlers In main.rs =============

/*
In main.rs:

use axum::{
    Router,
    routing::{get, post},
    middleware,
};

// Create a route group that requires authentication
let protected_routes = Router::new()
    .route("/api/profile", get(example_handlers::get_profile))
    .route("/api/upload", post(example_handlers::upload_file))
    .route("/api/files", get(example_handlers::list_user_files))
    .route("/api/files/:id", get(example_handlers::get_file))
    .route("/api/settings", post(example_handlers::update_settings))
    .layer(middleware::from_fn_with_state(
        app_config.clone(),
        oauth::auth_middleware
    ));

// Create routes that do not require authentication, or use optional authentication
let public_routes = Router::new()
    .route("/api/content", get(example_handlers::get_public_content));

// Merge all routes
let app = Router::new()
    .route("/api/oauth/login", get(oauth::login))
    .route("/api/oauth/callback", get(oauth::callback))
    .route("/api/logout", get(oauth::logout))
    .merge(public_routes)
    .merge(protected_routes)
    .with_state(app_config.clone())
    .fallback(static_content::serve_static);
*/
