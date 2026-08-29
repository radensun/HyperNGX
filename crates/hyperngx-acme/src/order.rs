//! Alur order ACME.

use crate::{CertRequest, Challenge, IssuedCert, KeyType};
use anyhow::{Context, Result};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, OrderStatus,
};
use std::path::Path;
use std::time::Duration;

/// Menerbitkan (atau memperpanjang) satu sertifikat.
///
/// Perpanjangan tidak dibedakan dari penerbitan baru: ACME tidak punya
/// operasi "renew", yang ada hanya order baru untuk himpunan domain yang
/// sama. Menyederhanakan ini menghilangkan satu jalur kode yang jarang
/// dieksekusi — dan jalur yang jarang dieksekusi adalah jalur yang rusak
/// tanpa ketahuan.
pub async fn issue(
    directory: &str,
    contact_email: Option<&str>,
    accounts_dir: &Path,
    webroot: &Path,
    req: &CertRequest,
) -> Result<IssuedCert> {
    req.validate()?;

    let account = load_or_create_account(directory, contact_email, accounts_dir).await?;

    let identifiers: Vec<Identifier> = req
        .domains
        .iter()
        .map(|d| Identifier::Dns(d.clone()))
        .collect();

    let mut order = account
        .new_order(&NewOrder { identifiers: &identifiers })
        .await
        .context("CA menolak order baru")?;

    let authorizations = order.authorizations().await?;
    let mut to_validate = Vec::new();

    for authz in &authorizations {
        if authz.status == AuthorizationStatus::Valid { continue; }

        let wanted = match &req.challenge {
            Challenge::Http01 => ChallengeType::Http01,
            Challenge::Dns01 { .. } => ChallengeType::Dns01,
        };
        let challenge = authz
            .challenges
            .iter()
            .find(|c| c.r#type == wanted)
            .context("CA tidak menawarkan challenge yang diminta")?;

        let Identifier::Dns(domain) = &authz.identifier;

        match &req.challenge {
            Challenge::Http01 => {
                let key_auth = order.key_authorization(challenge);
                write_http_token(webroot, &challenge.token, key_auth.as_str())?;
            }
            Challenge::Dns01 { provider } => {
                let digest = order.key_authorization(challenge).dns_value();
                crate::dns::publish(provider, domain, &digest).await?;
            }
        }
        to_validate.push(challenge.url.clone());
    }

    // DNS butuh waktu propagasi; memberi tahu CA terlalu cepat membakar
    // kuota kegagalan (5/jam/akun) tanpa perlu.
    if matches!(req.challenge, Challenge::Dns01 { .. }) {
        tokio::time::sleep(Duration::from_secs(20)).await;
    }

    for url in &to_validate {
        order.set_challenge_ready(url).await?;
    }

    wait_ready(&mut order).await?;

    // CSR dibuat lokal: private key tidak pernah meninggalkan server.
    let params = key_params(req)?;
    let key_pair = rcgen::KeyPair::generate_for(alg(req.key_type))?;
    let csr = params.serialize_request(&key_pair)?;

    order.finalize(csr.der()).await?;
    let fullchain = poll_certificate(&mut order).await?;

    // Bersihkan artefak challenge apa pun hasilnya.
    if let Challenge::Dns01 { provider } = &req.challenge {
        for d in &req.domains {
            let _ = crate::dns::cleanup(provider, d).await;
        }
    }

    let not_after = chrono::Utc::now() + chrono::Duration::days(90);
    Ok(IssuedCert {
        slug: req.slug.clone(),
        fullchain_pem: fullchain,
        private_key_pem: key_pair.serialize_pem(),
        not_after,
    })
}

fn alg(k: KeyType) -> &'static rcgen::SignatureAlgorithm {
    match k {
        KeyType::Ecdsa256 => &rcgen::PKCS_ECDSA_P256_SHA256,
        KeyType::Ecdsa384 => &rcgen::PKCS_ECDSA_P384_SHA384,
        KeyType::Rsa2048 => &rcgen::PKCS_RSA_SHA256,
    }
}

fn key_params(req: &CertRequest) -> Result<rcgen::CertificateParams> {
    let mut params = rcgen::CertificateParams::new(req.domains.clone())?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    Ok(params)
}

fn write_http_token(webroot: &Path, token: &str, key_auth: &str) -> Result<()> {
    let dir = webroot.join(".well-known/acme-challenge");
    std::fs::create_dir_all(&dir)?;
    // Nama berkas berasal dari CA, bukan dari pengguna, tapi tetap ditolak
    // bila mengandung pemisah path.
    anyhow::ensure!(
        !token.contains('/') && !token.contains(".."),
        "token ACME mengandung karakter path"
    );
    std::fs::write(dir.join(token), key_auth)?;
    Ok(())
}

async fn wait_ready(order: &mut instant_acme::Order) -> Result<()> {
    let mut delay = Duration::from_millis(500);
    for _ in 0..12 {
        tokio::time::sleep(delay).await;
        let state = order.refresh().await?;
        match state.status {
            OrderStatus::Ready => return Ok(()),
            OrderStatus::Invalid => anyhow::bail!("validasi domain gagal — periksa DNS dan port 80"),
            _ => delay = (delay * 2).min(Duration::from_secs(10)),
        }
    }
    anyhow::bail!("order tidak pernah mencapai status ready")
}

async fn poll_certificate(order: &mut instant_acme::Order) -> Result<String> {
    let mut delay = Duration::from_millis(500);
    for _ in 0..12 {
        if let Some(pem) = order.certificate().await? {
            return Ok(pem);
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(10));
    }
    anyhow::bail!("CA tidak mengirimkan sertifikat dalam batas waktu")
}

/// Kunci akun dipakai ulang antar penerbitan. Membuat akun baru setiap kali
/// adalah cara tercepat menabrak rate limit Let's Encrypt.
async fn load_or_create_account(
    directory: &str,
    contact_email: Option<&str>,
    accounts_dir: &Path,
) -> Result<Account> {
    std::fs::create_dir_all(accounts_dir)?;
    let path = accounts_dir.join(format!("{}.json", crate::store::directory_slug(directory)));

    if path.exists() {
        let creds = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        return Ok(Account::from_credentials(creds).await?);
    }

    let contact: Vec<String> = contact_email
        .map(|e| vec![format!("mailto:{e}")])
        .unwrap_or_default();
    let contact_refs: Vec<&str> = contact.iter().map(|s| s.as_str()).collect();

    let (account, creds) = Account::create(
        &NewAccount {
            contact: &contact_refs,
            terms_of_service_agreed: true,
            only_return_existing: false,
        },
        directory,
        None,
    )
    .await
    .context("pendaftaran akun ACME gagal")?;

    crate::store::write_secret(&path, &serde_json::to_vec_pretty(&creds)?)?;
    Ok(account)
}
