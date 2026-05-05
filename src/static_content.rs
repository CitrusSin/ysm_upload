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
    
    // Return index.html when the path is empty
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
            // Return index.html when the file does not exist, for SPA routes
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