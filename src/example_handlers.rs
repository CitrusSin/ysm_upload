// 示例：如何在新模块中使用 OAuth middleware
// 此文件展示了如何创建需要用户认证的 API

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::oauth::AuthUser;

// ============= 示例 1: 简单的用户资料 API =============

/// 获取当前用户的资料
/// 
/// 使用 AuthUser 参数自动获取认证用户信息
pub async fn get_profile(user: AuthUser) -> impl IntoResponse {
    Json(json!({
        "uid": user.uid,
        "nickname": user.nickname,
        "email": user.email,
        "players": user.players
    }))
}

// ============= 示例 2: 文件上传 API =============

/// 处理文件上传
/// 
/// 自动获取用户信息，并将文件与用户关联
pub async fn upload_file(
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut uploaded_files = Vec::new();
    
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap_or("unknown").to_string();
        let filename = field.file_name().unwrap_or("unnamed").to_string();
        let data = field.bytes().await.unwrap();
        
        // 这里添加你的文件保存逻辑
        // save_file_to_storage(user.uid, &filename, &data).await?;
        
        tracing::info!(
            "用户 {} (UID: {}) 上传了文件: {} ({} bytes)",
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

// ============= 示例 3: 带查询参数的 API =============

#[derive(Deserialize)]
pub struct FileListQuery {
    page: Option<u32>,
    limit: Option<u32>,
}

/// 列出用户的文件
/// 
/// 结合 Query 和 AuthUser 参数
pub async fn list_user_files(
    user: AuthUser,
    Query(query): Query<FileListQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(10);
    
    // 这里添加你的数据库查询逻辑
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
        "files": [] // 从数据库查询的文件列表
    }))
}

// ============= 示例 4: 带路径参数的 API =============

/// 获取特定文件
/// 
/// 包含权限检查：只有文件所有者才能访问
pub async fn get_file(
    user: AuthUser,
    Path(file_id): Path<u64>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // 这里添加你的数据库查询逻辑
    // let file = db.get_file(file_id).await
    //     .ok_or_else(|| (StatusCode::NOT_FOUND, "File not found".to_string()))?;
    
    // 权限检查示例
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

// ============= 示例 5: 带 State 的 API =============

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: String, // 实际应该是数据库连接池
}

/// 更新用户设置
/// 
/// 结合 State 和 AuthUser
pub async fn update_settings(
    user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    tracing::info!(
        "用户 {} (UID: {}) 正在更新设置",
        user.nickname,
        user.uid
    );
    
    // 这里添加你的数据库更新逻辑
    // db.update_user_settings(user.uid, &payload).await?;
    
    Ok(Json(json!({
        "success": true,
        "message": "Settings updated successfully",
        "user": user.nickname
    })))
}

// ============= 示例 6: 可选认证 =============

/// 获取公开内容
/// 
/// 支持可选认证：未登录用户也可以访问，但登录用户会看到个性化内容
pub async fn get_public_content(
    user: Option<AuthUser>, // 使用 Option 使认证变为可选
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

// ============= 如何在 main.rs 中使用这些 handler =============

/*
在 main.rs 中：

use axum::{
    Router,
    routing::{get, post},
    middleware,
};

// 创建需要认证的路由组
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

// 创建不需要认证的路由（或可选认证）
let public_routes = Router::new()
    .route("/api/content", get(example_handlers::get_public_content));

// 合并所有路由
let app = Router::new()
    .route("/api/oauth/login", get(oauth::login))
    .route("/api/oauth/callback", get(oauth::callback))
    .route("/api/logout", get(oauth::logout))
    .merge(public_routes)
    .merge(protected_routes)
    .with_state(app_config.clone())
    .fallback(static_content::serve_static);
*/
