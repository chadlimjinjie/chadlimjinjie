//! Users, passwords, browser sessions and one-time verification tokens.

use std::sync::OnceLock;

use argon2::{password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::http::{header, HeaderMap, HeaderValue};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{bad, internal, Result},
    mail,
    tenant::Tenant,
    AppState, Config,
};

const SESSION_COOKIE: &str = "auth_session";

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub email_verified: bool,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub name: String,
    pub suspended: bool,
    pub created_at: DateTime<Utc>,
}

/// 256 bits from the OS, base64url. Used for every bearer secret: sessions, codes, refresh tokens.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Secrets are high-entropy, so a plain SHA-256 is enough to store and look them up by.
pub fn sha256(s: &str) -> Vec<u8> {
    Sha256::digest(s.as_bytes()).to_vec()
}

pub fn normalize_email(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let ok = !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && email.len() <= 254
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    ok.then_some(email)
}

pub fn check_password(tenant: &Tenant, password: &str) -> Result<()> {
    let min = tenant.config.password_min_length;
    // the upper bound keeps argon2 from being fed arbitrarily large inputs
    if password.chars().count() < min || password.len() > 256 {
        return Err(bad(&format!("Password must be between {min} and 256 characters.")));
    }
    Ok(())
}

fn hash_sync(password: &str) -> std::result::Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(password.as_bytes(), &salt)?.to_string())
}

/// argon2id with the crate defaults (19 MiB, 2 passes).
pub async fn hash_password(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_sync(&password)).await.map_err(internal)?.map_err(internal)
}

/// With no stored hash, a dummy one is verified so unknown and passwordless accounts take as long as real ones.
pub async fn verify_password(password: String, hash: Option<String>) -> bool {
    static DUMMY: OnceLock<String> = OnceLock::new();
    tokio::task::spawn_blocking(move || {
        let known = hash.is_some();
        let hash = hash.unwrap_or_else(|| DUMMY.get_or_init(|| hash_sync("dummy").unwrap_or_default()).clone());
        let ok = PasswordHash::new(&hash)
            .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false);
        ok && known
    })
    .await
    .unwrap_or(false)
}

/// Returns the user only when the password matches and the account is not suspended.
pub async fn authenticate(db: &PgPool, tenant: &Tenant, email: &str, password: String) -> Result<Option<User>> {
    let user: Option<User> = match normalize_email(email) {
        Some(email) => {
            sqlx::query_as("select * from app_user where tenant_id = $1 and email = $2")
                .bind(tenant.id)
                .bind(email)
                .fetch_optional(db)
                .await?
        }
        None => None,
    };
    let hash = user.as_ref().and_then(|u| u.password_hash.clone());
    let ok = verify_password(password, hash).await;
    Ok(user.filter(|u| ok && !u.suspended))
}

pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// Where a cookie lives: the hosted pages on the gateway, or the session API that a tenant app proxies
/// on its own origin (host-only, so the app's domain owns it).
#[derive(Clone, Copy, PartialEq)]
pub enum Surface {
    Hosted,
    Api,
}

/// Hosted cookies are scoped to the tenant's path, so each tenant keeps its own browser session.
pub fn set_cookie(cfg: &Config, tenant: &Tenant, surface: Surface, name: &str, value: &str, max_age_secs: i64) -> HeaderValue {
    let (path, origin) = match surface {
        Surface::Hosted => (tenant.base(), cfg.public_url.as_str()),
        Surface::Api => ("/".to_string(), tenant.config.app_url.as_deref().unwrap_or_default()),
    };
    let secure = if origin.starts_with("https://") { "; Secure" } else { "" };
    let cookie = format!("{name}={value}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure}");
    HeaderValue::from_str(&cookie).unwrap_or(HeaderValue::from_static(""))
}

/// Creates a session and returns its `Set-Cookie` value.
pub async fn create_session(db: &PgPool, cfg: &Config, tenant: &Tenant, surface: Surface, user_id: Uuid, headers: &HeaderMap) -> Result<HeaderValue> {
    let token = random_token();
    let days = tenant.config.session_ttl_days;
    let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(|v| v.chars().take(300).collect::<String>());
    // display only: X-Forwarded-For is set by the gateway but can be spoofed by a client reaching the service directly
    let ip = text("x-forwarded-for").and_then(|v| v.split(',').next().map(|s| s.trim().to_string()));
    sqlx::query(
        "insert into session (token_hash, tenant_id, user_id, ip, user_agent, expires_at)
         values ($1, $2, $3, $4, $5, now() + make_interval(days => $6))",
    )
    .bind(sha256(&token))
    .bind(tenant.id)
    .bind(user_id)
    .bind(ip)
    .bind(text("user-agent"))
    .bind(days as i32)
    .execute(db)
    .await?;
    Ok(set_cookie(cfg, tenant, surface, SESSION_COOKIE, &token, days * 86_400))
}

/// The signed-in user for this tenant and when they authenticated.
pub async fn current_user(db: &PgPool, tenant: &Tenant, headers: &HeaderMap) -> Result<Option<(User, DateTime<Utc>)>> {
    let Some(token) = cookie(headers, SESSION_COOKIE) else { return Ok(None) };
    let row: Option<(DateTime<Utc>, Uuid)> = sqlx::query_as(
        "select s.created_at, s.user_id from session s join app_user u on u.id = s.user_id
         where s.token_hash = $1 and s.tenant_id = $2 and s.expires_at > now() and not u.suspended",
    )
    .bind(sha256(token))
    .bind(tenant.id)
    .fetch_optional(db)
    .await?;
    let Some((auth_time, user_id)) = row else { return Ok(None) };
    let user = sqlx::query_as("select * from app_user where id = $1").bind(user_id).fetch_one(db).await?;
    Ok(Some((user, auth_time)))
}

/// Deletes the current browser session and returns the cookie-clearing header.
pub async fn end_session(db: &PgPool, cfg: &Config, tenant: &Tenant, surface: Surface, headers: &HeaderMap) -> Result<HeaderValue> {
    if let Some(token) = cookie(headers, SESSION_COOKIE) {
        sqlx::query("delete from session where token_hash = $1 and tenant_id = $2")
            .bind(sha256(token))
            .bind(tenant.id)
            .execute(db)
            .await?;
    }
    Ok(set_cookie(cfg, tenant, surface, SESSION_COOKIE, "", 0))
}

/// Signs the user out everywhere: browser sessions and refresh tokens.
pub async fn revoke_user(db: &PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query("delete from session where user_id = $1").bind(user_id).execute(db).await?;
    sqlx::query("update refresh_token set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn create_verification(db: &PgPool, tenant_id: Uuid, user_id: Uuid, purpose: &str, ttl_hours: i32) -> Result<String> {
    let token = random_token();
    sqlx::query(
        "insert into verification (token_hash, tenant_id, user_id, purpose, expires_at)
         values ($1, $2, $3, $4, now() + make_interval(hours => $5))",
    )
    .bind(sha256(&token))
    .bind(tenant_id)
    .bind(user_id)
    .bind(purpose)
    .bind(ttl_hours)
    .execute(db)
    .await?;
    Ok(token)
}

/// Single use: the row is deleted as it is read.
pub async fn consume_verification(db: &PgPool, tenant_id: Uuid, token: &str, purpose: &str) -> Result<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "delete from verification where token_hash = $1 and tenant_id = $2 and purpose = $3 and expires_at > now()
         returning user_id",
    )
    .bind(sha256(token))
    .bind(tenant_id)
    .bind(purpose)
    .fetch_optional(db)
    .await?)
}

/// Mail links open the tenant app's own pages when it has one, otherwise the hosted pages.
fn mail_link(st: &AppState, tenant: &Tenant, path: &str, token: &str) -> String {
    let base = tenant.config.app_url.clone().unwrap_or_else(|| tenant.issuer(&st.cfg));
    format!("{base}/{path}?token={token}")
}

/// Registers a password account and mails a verification link. A taken address is a silent no-op,
/// so the outcome does not reveal who is registered. Bad input is a `BadRequest` with a user-facing message.
pub async fn register(st: &AppState, tenant: &Tenant, name: &str, email: &str, password: String) -> Result<()> {
    let email = normalize_email(email).ok_or_else(|| bad("Enter a valid email address."))?;
    check_password(tenant, &password)?;
    let hash = hash_password(password).await?;
    let created: Option<Uuid> = sqlx::query_scalar(
        "insert into app_user (tenant_id, email, name, password_hash) values ($1, $2, $3, $4)
         on conflict (tenant_id, email) do nothing returning id",
    )
    .bind(tenant.id)
    .bind(&email)
    .bind(name.trim().chars().take(200).collect::<String>())
    .bind(hash)
    .fetch_optional(&st.db)
    .await?;
    if let Some(user_id) = created {
        let token = create_verification(&st.db, tenant.id, user_id, "verify_email", 48).await?;
        let link = mail_link(st, tenant, "verify-email", &token);
        let (cfg, subject) = (st.cfg.clone(), format!("Verify your email for {}", tenant.name));
        tokio::spawn(async move { mail::send(&cfg, &email, &subject, &format!("Confirm your email address:\n\n{link}\n")).await });
    }
    Ok(())
}

/// Returns false when the link is invalid or expired.
pub async fn verify_email(st: &AppState, tenant: &Tenant, token: &str) -> Result<bool> {
    let Some(user_id) = consume_verification(&st.db, tenant.id, token, "verify_email").await? else { return Ok(false) };
    sqlx::query("update app_user set email_verified = true where id = $1").bind(user_id).execute(&st.db).await?;
    Ok(true)
}

/// Mails a reset link if the account exists; the caller always answers the same.
pub async fn send_reset(st: &AppState, tenant: &Tenant, email: &str) -> Result<()> {
    let Some(email) = normalize_email(email) else { return Ok(()) };
    let user_id: Option<Uuid> = sqlx::query_scalar("select id from app_user where tenant_id = $1 and email = $2 and not suspended")
        .bind(tenant.id)
        .bind(&email)
        .fetch_optional(&st.db)
        .await?;
    if let Some(user_id) = user_id {
        let token = create_verification(&st.db, tenant.id, user_id, "reset_password", 1).await?;
        let link = mail_link(st, tenant, "reset-password", &token);
        let (cfg, subject) = (st.cfg.clone(), format!("Reset your password for {}", tenant.name));
        // spawned so the response time does not reveal whether the account exists
        tokio::spawn(async move { mail::send(&cfg, &email, &subject, &format!("Reset your password (valid for one hour):\n\n{link}\n")).await });
    }
    Ok(())
}

/// Sets a new password and signs the user out everywhere. Returns false when the link is invalid or expired.
pub async fn reset_password(st: &AppState, tenant: &Tenant, token: &str, password: String) -> Result<bool> {
    // checked before the token is consumed, so a rejected password does not burn the link
    check_password(tenant, &password)?;
    let Some(user_id) = consume_verification(&st.db, tenant.id, token, "reset_password").await? else { return Ok(false) };
    let hash = hash_password(password).await?;
    // the link proved control of the mailbox, so the address counts as verified
    sqlx::query("update app_user set password_hash = $1, email_verified = true where id = $2")
        .bind(hash)
        .bind(user_id)
        .execute(&st.db)
        .await?;
    revoke_user(&st.db, user_id).await?;
    Ok(true)
}

/// The user's roles in this tenant as (name, scopes).
pub async fn roles(db: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<Vec<(String, Vec<String>)>> {
    Ok(sqlx::query_as("select r.name, r.scopes from role r join user_role ur on ur.role_id = r.id where ur.user_id = $1 and r.tenant_id = $2 order by r.name")
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(db)
        .await?)
}
