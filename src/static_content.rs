use axum::{
    extract::State,
    response::Redirect,
    response::{IntoResponse, Response},
    http::{StatusCode, header},
};
use rust_embed::RustEmbed;
use std::sync::Arc;

use crate::AppState;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

pub async fn serve_static(
    State(state): State<Arc<AppState>>,
    uri: axum::http::Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/');
    
    // Return index.html when the path is empty
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    match Assets::get(path) {
        Some(content) if path == "index.html" => serve_index(&state, content.data),
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
                Some(content) => serve_index(&state, content.data),
                None => {
                    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
                }
            }
        }
    }
}

pub async fn redirect_to_base(
    State(state): State<Arc<AppState>>,
) -> Redirect {
    Redirect::permanent(&state.frontend_base_href())
}

fn serve_index(state: &AppState, content: std::borrow::Cow<'static, [u8]>) -> Response {
    let html = String::from_utf8_lossy(content.as_ref());
    let injected_head = format!(
        "<head>\n    <base href=\"{}\" />\n    <script>window.__APP_BASE_PATH__ = {};</script>",
        state.frontend_base_href(),
        serde_json::to_string(state.frontend_base_path()).expect("base path should serialize")
    );

    let rendered = html.replace("<head>", &injected_head);

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        rendered,
    ).into_response()
}