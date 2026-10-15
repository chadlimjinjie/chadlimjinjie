//! auth-service — multi-tenant OIDC identity provider.
//!
//! Each tenant lives under `/auth/t/{slug}` and serves hosted sign-in pages plus the OIDC
//! endpoints. `/auth/api` is the management API used by the web-accounts console. All frontends
//! reach it through the APISIX gateway.

mod admin;
mod error;
mod identity;
mod keys;
mod mail;
mod oidc;
mod pages;
mod social;
mod tenant;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use axum::{
    http::{header, HeaderValue},
    routing::get,
    Json, Router,
};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing_subscriber::EnvFilter;

use crate::tenant::TenantConfig;

pub struct Config {
    /// Origin the gateway is reached on, without a trailing slash. Issuers and mail links are built
    /// from it, and hosted form posts must carry it as their Origin.
    pub public_url: String,
    /// Origin of the web-accounts console.
    pub console_url: String,
    /// Browser origins allowed to call the token, userinfo and management endpoints.
    pub cors_origins: Vec<String>,
    pub smtp_url: Option<String>,
    pub mail_from: String,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let origin = |name: &str, default: &str| env(name).unwrap_or_else(|| default.into()).trim_end_matches('/').to_string();
    let console_url = origin("CONSOLE_URL", "http://localhost:5174");
    let mut cors_origins = vec![console_url.clone()];
    cors_origins.extend(env("CORS_ORIGINS").iter().flat_map(|v| v.split(',')).map(|o| o.trim().trim_end_matches('/').to_string()));
    let cfg = Config {
        public_url: origin("PUBLIC_URL", "http://localhost:9080"),
        console_url,
        cors_origins,
        smtp_url: env("SMTP_URL"),
        mail_from: env("MAIL_FROM").unwrap_or_else(|| "auth@localhost".into()),
    };

    let database_url = env("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new().max_connections(10).connect(&database_url).await.expect("failed to connect to Postgres");
    sqlx::migrate!().run(&db).await.expect("failed to run migrations");

    let state = AppState { db, cfg: Arc::new(cfg) };
    bootstrap(&state, env("ADMIN_EMAIL"), env("ADMIN_PASSWORD")).await.expect("failed to bootstrap the admin tenant");
    tokio::spawn(sweep(state.db.clone()));

    let port = env("PORT").unwrap_or_else(|| "4112".to_string());
    let addr = format!("[::]:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!("auth-service listening on {addr}");
    axum::serve(listener, app(state)).await.unwrap();
}

pub fn app(state: AppState) -> Router {
    let origins = state.cfg.cors_origins.iter().filter_map(|o| o.parse::<HeaderValue>().ok());
    // Bearer tokens only, never cookies, so credentials stay disallowed.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(Any)
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    let tenant = pages::router(state.clone()).merge(oidc::router().layer(cors.clone()));
    Router::new()
        .route("/healthz", get(health))
        .nest("/auth/t/{tenant}", tenant)
        .nest("/auth/api", admin::router(state.clone()).layer(cors))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "auth-service" }))
}

/// First start: the admin tenant, the console's OIDC client, the admin role and one operator.
/// Later starts: while ADMIN_PASSWORD is set, that operator is restored with it (password, admin role,
/// not suspended), which is also the way back in after a lockout. Unset it to manage the password in the console.
pub async fn bootstrap(st: &AppState, email: Option<String>, password: Option<String>) -> error::Result<()> {
    let existing = tenant::Tenant::by_slug(&st.db, admin::ADMIN_TENANT).await?;
    if existing.is_some() && password.is_none() {
        return Ok(());
    }
    let email = identity::normalize_email(email.as_deref().unwrap_or("admin@localhost")).ok_or_else(|| error::bad("ADMIN_EMAIL is invalid"))?;
    // No default credentials: without ADMIN_PASSWORD a random one is generated and logged once.
    let generated = password.is_none();
    let password = password.unwrap_or_else(|| identity::random_token()[..20].to_string());
    let hash = identity::hash_password(password.clone()).await?;

    // One transaction, so a crash cannot leave an admin tenant nobody can sign in to.
    let mut tx = st.db.begin().await?;
    let tenant = match existing {
        Some(tenant) => tenant,
        None => {
            let config = TenantConfig { sign_up_enabled: false, ..Default::default() };
            let tenant = tenant::create(&mut tx, admin::ADMIN_TENANT, "Admin", config).await?;
            sqlx::query("insert into oidc_client (tenant_id, client_id, name, redirect_uris, post_logout_uris) values ($1, 'web-accounts', 'Accounts console', $2, $3)")
                .bind(tenant.id)
                .bind(vec![format!("{}/callback", st.cfg.console_url)])
                .bind(vec![st.cfg.console_url.clone()])
                .execute(&mut *tx)
                .await?;
            tenant
        }
    };
    sqlx::query(
        "with r as (insert into role (tenant_id, name, description, scopes) values ($1, 'admin', 'Full access to the management API', $2)
                    on conflict (tenant_id, name) do update set scopes = excluded.scopes returning id),
              u as (insert into app_user (tenant_id, email, name, password_hash, email_verified) values ($1, $3, 'Administrator', $4, true)
                    on conflict (tenant_id, email) do update set password_hash = excluded.password_hash, suspended = false returning id)
         insert into user_role (user_id, role_id) select u.id, r.id from u, r on conflict do nothing",
    )
    .bind(tenant.id)
    .bind(vec![admin::MANAGE_SCOPE])
    .bind(&email)
    .bind(hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    if generated {
        tracing::warn!("created console operator {email} with generated password: {password} (change it after first sign-in)");
    } else {
        tracing::info!("console operator {email} uses the password from ADMIN_PASSWORD");
    }
    Ok(())
}

/// Hourly removal of rows that can no longer be used.
async fn sweep(db: PgPool) {
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tick.tick().await;
        for table in ["session", "verification", "auth_code", "refresh_token"] {
            // codes and refresh tokens linger a day past expiry so replay detection can still find their family
            let sql = format!("delete from {table} where expires_at < now() - interval '1 day'");
            if let Err(e) = sqlx::query(&sql).execute(&db).await {
                tracing::error!("sweep of {table} failed: {e}");
            }
        }
    }
}
