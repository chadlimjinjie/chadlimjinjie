//! Per-tenant RS256 signing keys and JWT helpers.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use rsa::{
    pkcs8::{EncodePrivateKey, LineEnding},
    traits::PublicKeyParts,
    RsaPrivateKey,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    error::{internal, Result},
    tenant::Tenant,
    AppState,
};

/// Creates a new active key and retires the previous one. Retired keys stop verifying immediately,
/// so rotation also works as the response to a leaked key; clients recover through their refresh token.
pub async fn generate(conn: &mut PgConnection, tenant_id: Uuid) -> Result<()> {
    let (pem, n, e) = tokio::task::spawn_blocking(|| -> std::result::Result<_, String> {
        let key = RsaPrivateKey::new(&mut OsRng, 2048).map_err(|e| e.to_string())?;
        let pem = key.to_pkcs8_pem(LineEnding::LF).map_err(|e| e.to_string())?.to_string();
        Ok((pem, URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()), URL_SAFE_NO_PAD.encode(key.e().to_bytes_be())))
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;

    sqlx::query("update signing_key set retired_at = now() where tenant_id = $1 and retired_at is null")
        .bind(tenant_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("insert into signing_key (tenant_id, private_pem, n, e) values ($1, $2, $3, $4)")
        .bind(tenant_id)
        .bind(pem)
        .bind(n)
        .bind(e)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn sign(db: &PgPool, tenant_id: Uuid, claims: &Value) -> Result<String> {
    let (kid, pem): (Uuid, String) =
        sqlx::query_as("select kid, private_pem from signing_key where tenant_id = $1 and retired_at is null")
            .bind(tenant_id)
            .fetch_one(db)
            .await?;
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let key = EncodingKey::from_rsa_pem(pem.as_bytes()).map_err(internal)?;
    encode(&header, claims, &key).map_err(internal)
}

/// Verifies a JWT against this tenant's active key. The key is looked up by tenant, so a token
/// signed by another tenant never verifies here.
pub async fn verify<T: DeserializeOwned>(db: &PgPool, tenant_id: Uuid, token: &str, validation: &Validation) -> Option<T> {
    let kid: Uuid = decode_header(token).ok()?.kid?.parse().ok()?;
    let (n, e): (String, String) =
        sqlx::query_as("select n, e from signing_key where kid = $1 and tenant_id = $2 and retired_at is null")
            .bind(kid)
            .bind(tenant_id)
            .fetch_optional(db)
            .await
            .ok()??;
    let key = DecodingKey::from_rsa_components(&n, &e).ok()?;
    decode::<T>(token, &key, validation).ok().map(|d| d.claims)
}

#[derive(Deserialize)]
pub struct AccessClaims {
    pub sub: Uuid,
    pub scope: String,
}

impl AccessClaims {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }
}

/// Access tokens carry `aud = issuer`; ID tokens carry `aud = client_id`, so they are rejected here.
pub async fn verify_access(st: &AppState, tenant: &Tenant, token: &str) -> Option<AccessClaims> {
    let issuer = tenant.issuer(&st.cfg);
    let mut v = Validation::new(Algorithm::RS256);
    v.set_issuer(&[&issuer]);
    v.set_audience(&[&issuer]);
    verify(&st.db, tenant.id, token, &v).await
}

pub async fn jwks(db: &PgPool, tenant_id: Uuid) -> Result<Value> {
    let keys: Vec<(Uuid, String, String)> =
        sqlx::query_as("select kid, n, e from signing_key where tenant_id = $1 and retired_at is null")
            .bind(tenant_id)
            .fetch_all(db)
            .await?;
    let keys: Vec<Value> = keys
        .into_iter()
        .map(|(kid, n, e)| json!({ "kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid, "n": n, "e": e }))
        .collect();
    Ok(json!({ "keys": keys }))
}
