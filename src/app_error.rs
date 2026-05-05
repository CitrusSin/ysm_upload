use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError(pub anyhow::Error);

impl AsRef<anyhow::Error> for AppError {
    fn as_ref(&self) -> &anyhow::Error {
        &self.0
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(error = ?self.0, "Request failed");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}