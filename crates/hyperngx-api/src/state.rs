use crate::supervisor_client::SupervisorClient;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub supervisor: Arc<SupervisorClient>,
    /// Penghitung percobaan login, per (IP, username).
    pub login_attempts: Arc<tokio::sync::Mutex<crate::auth::AttemptTracker>>,
}
