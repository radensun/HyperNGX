//! hyperngx-core — model domain, validasi, dan rendering konfigurasi nginx.
//!
//! Crate ini murni (tanpa I/O jaringan, tanpa privilege) agar bisa diuji
//! sebagai fungsi: (state di DB) -> (bundle konfigurasi nginx).

pub mod apply;
pub mod error;
pub mod model;
pub mod render;
pub mod validate;

pub use error::{CoreError, Result};
