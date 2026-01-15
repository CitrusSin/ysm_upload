use axum::{
    response::{IntoResponse, Response},
    http::{StatusCode, header},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

pub async fn serve_static(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    
    // 如果路径为空，返回 index.html
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data,
            ).into_response()
        }
        None => {
            // 如果文件不存在，返回 index.html (用于 SPA 路由)
            match Assets::get("index.html") {
                Some(content) => {
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "text/html")],
                        content.data,
                    ).into_response()
                }
                None => {
                    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
                }
            }
        }
    }
}