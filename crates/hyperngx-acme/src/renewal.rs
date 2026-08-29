use chrono::{DateTime, Duration, Utc};

/// Kebijakan perpanjangan.
///
/// Let's Encrypt menerbitkan sertifikat 90 hari. Kita memperbarui pada
/// sisa 30 hari, dengan jitter acak 0–12 jam supaya ribuan instance
/// HyperNGX tidak menyerbu CA pada jam yang sama (thundering herd),
/// dan rate limit CA (50 sertifikat/domain/minggu) tetap aman.
pub struct RenewalPolicy {
    pub renew_before: Duration,
    pub max_jitter: Duration,
    pub max_attempts_per_day: u32,
}

impl Default for RenewalPolicy {
    fn default() -> Self {
        Self {
            renew_before: Duration::days(30),
            max_jitter: Duration::hours(12),
            max_attempts_per_day: 5,
        }
    }
}

impl RenewalPolicy {
    pub fn due(&self, not_after: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        now >= not_after - self.renew_before
    }
}

/// Backoff eksponensial saat CA menolak (rate limit / validasi gagal).
pub fn backoff_secs(attempt: u32) -> u64 {
    let base = 60u64.saturating_mul(2u64.saturating_pow(attempt.min(8)));
    base.min(24 * 3600)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn perpanjang_pada_sisa_30_hari() {
        let p = RenewalPolicy::default();
        let now = Utc::now();
        assert!(p.due(now + Duration::days(29), now));
        assert!(!p.due(now + Duration::days(31), now));
    }
}
