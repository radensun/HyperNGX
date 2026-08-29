//! Validasi input di batas API.
//!
//! Validasi yang sama dijalankan lagi oleh supervisor sebelum render.
//! Duplikasi ini disengaja: yang di sini memberi pesan error yang enak
//! dibaca di UI, yang di sana adalah pertahanan sesungguhnya — sebab
//! supervisor tidak boleh mempercayai pemanggilnya.

use crate::error::ApiError;
use hyperngx_core::validate;

pub fn domains(input: &[String]) -> Result<Vec<String>, ApiError> {
    if input.is_empty() {
        return Err(ApiError::BadRequest("Masukkan minimal satu domain.".into()));
    }
    input.iter()
        .map(|d| validate::normalize_domain(d)
            .map_err(|_| ApiError::BadRequest(format!("Domain tidak valid: {d}"))))
        .collect()
}

pub fn advanced(raw: &str) -> Result<(), ApiError> {
    validate::scan_advanced(raw).map_err(|e| ApiError::BadRequest(
        format!("Konfigurasi lanjutan ditolak: {e}")
    ))
}
