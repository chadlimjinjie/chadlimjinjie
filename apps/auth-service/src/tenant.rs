//! Tenants: fully isolated identity spaces, addressed by `/auth/t/{slug}`.

use axum::{
    extract::{FromRequestParts, RawPathParams},
    http::request::Parts,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::Json, PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    error::{bad, AppError, Result},
    keys, AppState, Config,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TenantConfig {
    pub sign_up_enabled: bool,
    pub password_min_length: usize,
    pub session_ttl_days: i64,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_days: i64,
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            sign_up_enabled: true,
            password_min_length: 8,
            session_ttl_days: 14,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_days: 14,
        }
    }
}

impl TenantConfig {
    pub fn validate(&self) -> Result<()> {
        let ok = (8..=128).contains(&self.password_min_length)
            && (1..=365).contains(&self.session_ttl_days)
            && (60..=86_400).contains(&self.access_token_ttl_secs)
            && (1..=365).contains(&self.refresh_token_ttl_days);
        ok.then_some(()).ok_or_else(|| bad("config value out of range"))
    }
}

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Tenant {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub config: Json<TenantConfig>,
    pub created_at: DateTime<Utc>,
}

impl Tenant {
    pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Option<Tenant>> {
        Ok(sqlx::query_as("select * from tenant where slug = $1").bind(slug).fetch_optional(db).await?)
    }

    /// OIDC issuer, and the base URL of everything this tenant serves.
    pub fn issuer(&self, cfg: &Config) -> String {
        format!("{}{}", cfg.public_url, self.base())
    }

    pub fn base(&self) -> String {
        format!("/auth/t/{}", self.slug)
    }
}

/// Creates a tenant together with its first signing key.
pub async fn create(conn: &mut PgConnection, slug: &str, name: &str, config: TenantConfig) -> Result<Tenant> {
    config.validate()?;
    let tenant: Tenant = sqlx::query_as("insert into tenant (slug, name, config) values ($1, $2, $3) returning *")
        .bind(slug)
        .bind(name.trim())
        .bind(Json(config))
        .fetch_one(&mut *conn)
        .await?;
    keys::generate(conn, tenant.id).await?;
    Ok(tenant)
}

/// Resolves the `{tenant}` path segment; unknown slugs are a 404.
impl FromRequestParts<AppState> for Tenant {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self> {
        let params = RawPathParams::from_request_parts(parts, state).await.map_err(|_| AppError::NotFound)?;
        let slug = params.iter().find(|(k, _)| *k == "tenant").map(|(_, v)| v).ok_or(AppError::NotFound)?;
        Tenant::by_slug(&state.db, slug).await?.ok_or(AppError::NotFound)
    }
}
