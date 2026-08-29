pub mod auth_routes;
pub mod certs_routes;
pub mod hosts_routes;
pub mod system_routes;

use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;

/// Permukaan REST. Semua endpoint tulis:
///   * memerlukan sesi valid + CSRF token (ditegakkan extractor CurrentUser)
///   * dicatat ke tabel audit_log (siapa, kapan, diff apa)
///   * memicu ApplyConfig ke supervisor, bukan menulis berkas sendiri
pub fn build(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/logout", post(auth_routes::logout))
        .route("/auth/me", get(auth_routes::me))
        .route("/auth/totp/enroll", post(auth_routes::totp_enroll))
        .route("/auth/totp/confirm", post(auth_routes::totp_confirm))
        .route("/hosts", get(hosts_routes::list).post(hosts_routes::create))
        .route("/hosts/{id}",
               get(hosts_routes::detail)
               .put(hosts_routes::update)
               .delete(hosts_routes::delete))
        .route("/hosts/{id}/toggle", post(hosts_routes::toggle))
        .route("/certificates", get(certs_routes::list).post(certs_routes::request))
        .route("/certificates/{slug}/renew", post(certs_routes::renew))
        .route("/certificates/{slug}", axum::routing::delete(certs_routes::revoke))
        .route("/config/dry-run", post(system_routes::dry_run))
        .route("/config/rollback", post(system_routes::rollback))
        .route("/generations", get(system_routes::generations))
        .route("/audit", get(system_routes::audit))
        .route("/status", get(system_routes::status))
        .route("/health", get(|| async { "ok" }))
        .with_state(state);

    Router::new()
        .nest("/api/v1", api)
        // SPA: semua path lain dilayani index.html agar routing sisi klien
        // bekerja saat halaman di-refresh.
        .fallback_service(
            tower_http::services::ServeDir::new("/usr/share/hyperngx/web")
                .fallback(tower_http::services::ServeFile::new("/usr/share/hyperngx/web/index.html")),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::limit::RequestBodyLimitLayer::new(1024 * 1024))
}
