use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("domain tidak valid: {0}")]
    InvalidDomain(String),
    #[error("upstream tidak valid: {0}")]
    InvalidUpstream(String),
    #[error("konfigurasi lanjutan ditolak: {0}")]
    UnsafeDirective(String),
    #[error("render template gagal: {0}")]
    Render(#[from] minijinja::Error),
    #[error("nginx -t gagal:\n{0}")]
    NginxTestFailed(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
