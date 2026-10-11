//! JWT verification against the Go auth-service's JWKS, mirroring
//! `connect-four-service/internal/auth/jwks.go`. The public JWKS is fetched from
//! `<AUTH_SERVICE_URL>/api/auth/jwks` and cached, refreshed periodically in the
//! background. A signed-in player's WS connection carries the token as a query
//! param (browsers can't set WS headers); an invalid token falls back to anon.

use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

struct AuthState {
    jwks_url: String,
    keys: RwLock<Option<JwkSet>>,
}

static STATE: OnceLock<Arc<AuthState>> = OnceLock::new();

/// A verified account identity (only the fields we need).
pub struct User {
    pub id: String,
}

#[derive(Deserialize)]
struct Claims {
    // better-auth / auth-service put the user id in a private `id` claim; fall
    // back to the standard `sub` when absent.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    sub: Option<String>,
    // Remaining claims (exp/nbf) are validated by the library, not read here.
}

/// Register the JWKS endpoint and warm the cache, then spawn a refresh loop.
/// Non-fatal on failure at startup — verification retries against a later cache.
pub fn init(auth_service_url: &str) {
    let jwks_url = format!("{}/api/auth/jwks", auth_service_url.trim_end_matches('/'));
    let state = Arc::new(AuthState {
        jwks_url,
        keys: RwLock::new(None),
    });
    let _ = STATE.set(state.clone());

    tokio::spawn(async move {
        loop {
            match fetch_jwks(&state.jwks_url).await {
                Ok(set) => {
                    if let Ok(mut guard) = state.keys.write() {
                        *guard = Some(set);
                    }
                }
                Err(e) => tracing::warn!("sudoku: failed to refresh JWKS: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    });
}

async fn fetch_jwks(url: &str) -> Result<JwkSet, String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let set = resp.json::<JwkSet>().await.map_err(|e| e.to_string())?;
    Ok(set)
}

/// Verify an ES256 bearer token against the cached JWKS, returning the user.
pub fn verify_token(token: &str) -> Result<User, String> {
    let state = STATE.get().ok_or("auth not initialized")?;

    let header = decode_header(token).map_err(|e| format!("bad header: {e}"))?;
    let kid = header.kid.ok_or("token missing kid")?;

    let guard = state.keys.read().map_err(|_| "jwks lock poisoned")?;
    let set = guard.as_ref().ok_or("jwks not loaded yet")?;
    let jwk = set.find(&kid).ok_or("no matching jwk for kid")?;

    let key = DecodingKey::from_jwk(jwk).map_err(|e| format!("decode key: {e}"))?;
    let mut validation = Validation::new(Algorithm::ES256);
    // We only care that the token is well-formed and unexpired; audience varies.
    validation.validate_aud = false;

    let data = decode::<Claims>(token, &key, &validation).map_err(|e| format!("verify: {e}"))?;
    let id = data
        .claims
        .id
        .or(data.claims.sub)
        .ok_or("missing id/sub claim")?;
    Ok(User { id })
}
