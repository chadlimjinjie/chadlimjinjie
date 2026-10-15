//! Headless session API, for tenants that build their own sign-in UI.
//!
//! The tenant app proxies `/api/auth/*` on its own origin to `/auth/t/{slug}/session/*`, so the session
//! cookie is first-party and host-only on the app's domain. Backends get a short-lived access token from
//! `/token` and verify it against the tenant's JWKS, exactly like OIDC access tokens.

use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    error::{AppError, Result},
    identity::{self, Surface, User},
    keys,
    social::{self, Callback, ProviderPath},
    tenant::Tenant,
    AppState,
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/sign-up", post(sign_up))
        .route("/sign-in", post(sign_in))
        .route("/sign-out", post(sign_out))
        .route("/me", get(me))
        .route("/token", get(token))
        .route("/verify-email", post(verify_email))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        .route("/social/{provider}", get(social_start))
        .route("/social/{provider}/callback", get(social_callback))
        .layer(middleware::from_fn_with_state(state, guard))
}

/// CSRF defence: anything but a GET must carry one of the tenant's session origins. Browsers send Origin
/// on every cross-origin and same-origin POST, and the app's proxy passes it through.
async fn guard(tenant: Tenant, req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let allowed = origin.is_some_and(|o| tenant.config.session_origins().any(|a| a == o));
    if req.method() != Method::GET && !allowed {
        return AppError::Forbidden.into_response();
    }
    let mut res = next.run(req).await;
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn ok() -> Json<Value> {
    Json(json!({ "ok": true }))
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

async fn user_json(st: &AppState, tenant: &Tenant, user: &User) -> Result<Value> {
    let roles: Vec<String> = identity::roles(&st.db, tenant.id, user.id).await?.into_iter().map(|(name, _)| name).collect();
    Ok(json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "email_verified": user.email_verified,
        "roles": roles,
    }))
}

async fn signed_in_user(st: &AppState, tenant: &Tenant, headers: &HeaderMap) -> Result<User> {
    let (user, _) = identity::current_user(&st.db, tenant, headers).await?.ok_or(AppError::Unauthorized)?;
    Ok(user)
}

#[derive(Deserialize)]
struct SignUp {
    #[serde(default)]
    name: String,
    email: String,
    password: String,
}

/// Same answer whether or not the address was already registered; the app signs in next.
async fn sign_up(State(st): State<AppState>, tenant: Tenant, Json(b): Json<SignUp>) -> Result<Json<Value>> {
    if !tenant.config.sign_up_enabled {
        return Err(AppError::Forbidden);
    }
    identity::register(&st, &tenant, &b.name, &b.email, b.password).await?;
    Ok(ok())
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

async fn sign_in(State(st): State<AppState>, tenant: Tenant, headers: HeaderMap, Json(b): Json<Credentials>) -> Result<Response> {
    // one message for unknown email, wrong password and suspended account
    let Some(user) = identity::authenticate(&st.db, &tenant, &b.email, b.password).await? else {
        return Ok(error(StatusCode::UNAUTHORIZED, "Invalid email or password."));
    };
    let cookie = identity::create_session(&st.db, &st.cfg, &tenant, Surface::Api, user.id, &headers).await?;
    Ok(([(header::SET_COOKIE, cookie)], Json(user_json(&st, &tenant, &user).await?)).into_response())
}

async fn sign_out(State(st): State<AppState>, tenant: Tenant, headers: HeaderMap) -> Result<Response> {
    let cookie = identity::end_session(&st.db, &st.cfg, &tenant, Surface::Api, &headers).await?;
    Ok(([(header::SET_COOKIE, cookie)], ok()).into_response())
}

async fn me(State(st): State<AppState>, tenant: Tenant, headers: HeaderMap) -> Result<Json<Value>> {
    let user = signed_in_user(&st, &tenant, &headers).await?;
    Ok(Json(user_json(&st, &tenant, &user).await?))
}

/// A short-lived RS256 access token for the tenant's backends, with the scopes of all the user's roles.
async fn token(State(st): State<AppState>, tenant: Tenant, headers: HeaderMap) -> Result<Json<Value>> {
    let user = signed_in_user(&st, &tenant, &headers).await?;
    let roles = identity::roles(&st.db, tenant.id, user.id).await?;
    let mut scopes: Vec<&str> = roles.iter().flat_map(|(_, s)| s.iter().map(String::as_str)).collect();
    scopes.sort_unstable();
    scopes.dedup();
    let issuer = tenant.issuer(&st.cfg);
    let now = Utc::now().timestamp();
    let ttl = tenant.config.access_token_ttl_secs;
    let claims = json!({
        "iss": issuer,
        "sub": user.id,
        "aud": issuer,
        "client_id": "session",
        "tenant": tenant.slug,
        "scope": scopes.join(" "),
        "roles": roles.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "email": user.email,
        "name": user.name,
        "iat": now,
        "exp": now + ttl,
    });
    Ok(Json(json!({ "access_token": keys::sign(&st.db, tenant.id, &claims).await?, "token_type": "Bearer", "expires_in": ttl })))
}

#[derive(Deserialize)]
struct Token {
    token: String,
}

async fn verify_email(State(st): State<AppState>, tenant: Tenant, Json(b): Json<Token>) -> Result<Response> {
    if !identity::verify_email(&st, &tenant, &b.token).await? {
        return Ok(error(StatusCode::BAD_REQUEST, "This link is invalid or has expired."));
    }
    Ok(ok().into_response())
}

#[derive(Deserialize)]
struct Email {
    email: String,
}

async fn forgot_password(State(st): State<AppState>, tenant: Tenant, Json(b): Json<Email>) -> Result<Json<Value>> {
    identity::send_reset(&st, &tenant, &b.email).await?;
    Ok(ok())
}

#[derive(Deserialize)]
struct NewPassword {
    token: String,
    password: String,
}

async fn reset_password(State(st): State<AppState>, tenant: Tenant, Json(b): Json<NewPassword>) -> Result<Response> {
    if !identity::reset_password(&st, &tenant, &b.token, b.password).await? {
        return Ok(error(StatusCode::BAD_REQUEST, "This link is invalid or has expired."));
    }
    Ok(ok().into_response())
}

/// Only pages of the tenant's own app are valid targets, which rules out open redirects.
fn safe_return<'a>(tenant: &Tenant, return_to: Option<&'a str>) -> Option<&'a str> {
    let app = tenant.config.app_url.as_deref()?;
    return_to.filter(|r| *r == app || r.starts_with(&format!("{app}/")))
}

#[derive(Deserialize)]
struct ReturnTo {
    return_to: Option<String>,
}

async fn social_start(State(st): State<AppState>, tenant: Tenant, Path(path): Path<ProviderPath>, Query(q): Query<ReturnTo>) -> Result<Response> {
    // without app_url there is no proxied callback to come back to
    let app = tenant.config.app_url.as_deref().ok_or(AppError::NotFound)?;
    let return_to = safe_return(&tenant, q.return_to.as_deref()).unwrap_or(app);
    social::begin(&st, &tenant, &path.provider, return_to, Surface::Api).await
}

async fn social_callback(
    State(st): State<AppState>,
    tenant: Tenant,
    Path(path): Path<ProviderPath>,
    Query(q): Query<Callback>,
    headers: HeaderMap,
) -> Result<Response> {
    let app = tenant.config.app_url.clone().ok_or(AppError::NotFound)?;
    let mut res = match social::finish(&st, &tenant, &path.provider, q, &headers, Surface::Api).await? {
        Ok((user, return_to)) => {
            let target = safe_return(&tenant, Some(&return_to)).unwrap_or(&app).to_string();
            let cookie = identity::create_session(&st.db, &st.cfg, &tenant, Surface::Api, user.id, &headers).await?;
            let mut res = Redirect::to(&target).into_response();
            res.headers_mut().append(header::SET_COOKIE, cookie);
            res
        }
        Err(why) => {
            let why: String = url::form_urlencoded::byte_serialize(why.as_bytes()).collect();
            Redirect::to(&format!("{app}/sign-in?error={why}")).into_response()
        }
    };
    res.headers_mut().append(header::SET_COOKIE, social::clear_state(&st, &tenant, Surface::Api));
    Ok(res)
}
