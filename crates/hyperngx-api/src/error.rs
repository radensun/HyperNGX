use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("tidak terautentikasi")]
    Unauthorized,
    #[error("tidak berwenang untuk tindakan ini")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("tidak ditemukan")]
    NotFound,
    #[error("terlalu banyak percobaan, coba lagi nanti")]
    TooManyRequests,
    #[error("konfigurasi ditolak: {0}")]
    ConfigRejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            ApiError::ConfigRejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Db(_) | ApiError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Detail internal tidak pernah sampai ke klien: pesan database bisa
        // membocorkan nama tabel dan struktur query.
        let message = match &self {
            ApiError::Db(e) => {
                tracing::error!(error = %e, "kesalahan database");
                "Terjadi kesalahan internal.".to_string()
            }
            ApiError::Other(e) => {
                tracing::error!(error = %e, "kesalahan internal");
                "Terjadi kesalahan internal.".to_string()
            }
            other => other.to_string(),
        };

        (status, Json(serde_json::json!({ "message": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
