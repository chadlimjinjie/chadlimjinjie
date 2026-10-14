//! Hosted sign-in experience: server-rendered forms, same origin as the session cookie.

use askama::Template;
use axum::{
    extract::{FromRequestParts, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Form, Router,
};
use serde::Deserialize;
use url::Url;
use uuid::Uuid;

use crate::{
    error::{internal, AppError, Result},
    identity, mail, social,
    tenant::Tenant,
    AppState,
};

pub struct Field {
    name: &'static str,
    label: &'static str,
    kind: &'static str,
    autocomplete: &'static str,
}

const EMAIL: Field = Field { name: "email", label: "Email", kind: "email", autocomplete: "username" };
const PASSWORD: Field = Field { name: "password", label: "Password", kind: "password", autocomplete: "current-password" };
const NEW_PASSWORD: Field = Field { name: "password", label: "New password", kind: "password", autocomplete: "new-password" };
const NAME: Field = Field { name: "name", label: "Name", kind: "text", autocomplete: "name" };

/// The one template behind every hosted page; a page without `submit` is a plain message.
#[derive(Template, Default)]
#[template(path = "page.html")]
pub struct Page<'a> {
    pub tenant: &'a str,
    pub title: &'a str,
    pub error: Option<&'a str>,
    pub notice: Option<&'a str>,
    pub fields: &'a [Field],
    pub submit: Option<&'a str>,
    pub socials: Vec<(String, String)>,
    pub links: Vec<(String, &'a str)>,
}

impl Page<'_> {
    pub fn respond(self, status: StatusCode) -> Response {
        match self.render() {
            Ok(body) => (status, Html(body)).into_response(),
            Err(e) => internal(e).into_response(),
        }
    }
}

#[derive(Deserialize)]
pub struct ReturnTo {
    pub return_to: Option<String>,
}

impl ReturnTo {
    /// Only this tenant's own authorize endpoint is a valid target, which rules out open redirects.
    pub fn safe(&self, tenant: &Tenant) -> Option<&str> {
        self.return_to.as_deref().filter(|r| r.starts_with(&format!("{}/authorize?", tenant.base())))
    }
}

pub fn with_return(path: String, return_to: Option<&str>) -> String {
    match return_to {
        Some(r) => format!("{path}?return_to={}", url::form_urlencoded::byte_serialize(r.as_bytes()).collect::<String>()),
        None => path,
    }
}

/// A tenant with its own login UI shows every page there instead: `{login_ui_url}/{page}?{params}`.
/// Only codes cross over, never text, so a crafted link cannot put words on that UI.
pub fn ui(tenant: &Tenant, page: &str, params: &[(&str, Option<&str>)]) -> Option<Response> {
    let mut url = Url::parse(&format!("{}/{page}", tenant.config.login_ui_url.as_deref()?)).ok()?;
    let pairs: Vec<_> = params.iter().filter_map(|(k, v)| v.map(|v| (*k, v))).collect();
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Some(Redirect::to(url.as_str()).into_response())
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/sign-in", get(sign_in_form).post(sign_in))
        .route("/sign-up", get(sign_up_form).post(sign_up))
        .route("/verify-email", get(verify_email))
        .route("/forgot-password", get(forgot_form).post(forgot))
        .route("/reset-password", get(reset_form).post(reset))
        .merge(social::router())
        .layer(middleware::from_fn_with_state(state, guard))
}

/// CSRF defence for the cookie-authenticated forms: a POST must carry our own Origin, or the tenant's
/// login UI origin, which browsers always send on form posts and pages on other sites cannot forge.
/// Also sets the page security headers.
async fn guard(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok()).map(str::to_string);
    let req = if req.method() == Method::POST && origin.as_deref() != Some(st.cfg.public_url.as_str()) {
        let (mut parts, body) = req.into_parts();
        let login_ui = Tenant::from_request_parts(&mut parts, &st).await.ok().and_then(|t| t.config.0.login_ui_url);
        if origin.is_none() || origin != login_ui {
            return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
        }
        Request::from_parts(parts, body)
    } else {
        req
    };
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'"),
    );
    res
}

async fn sign_in_page(st: &AppState, tenant: &Tenant, q: &ReturnTo, error: Option<&str>) -> Result<Response> {
    let return_to = q.safe(tenant);
    if let Some(r) = ui(tenant, "sign-in", &[("error", error.map(|_| "invalid_credentials")), ("return_to", return_to)]) {
        return Ok(r);
    }
    let base = tenant.base();
    let providers: Vec<String> =
        sqlx::query_scalar("select provider from social_connector where tenant_id = $1 and enabled order by provider")
            .bind(tenant.id)
            .fetch_all(&st.db)
            .await?;
    let socials = providers
        .into_iter()
        .map(|p| (with_return(format!("{base}/social/{p}"), return_to), format!("Continue with {}", social::label(&p))))
        .collect();
    let mut links = vec![(with_return(format!("{base}/forgot-password"), return_to), "Forgot password?")];
    if tenant.config.sign_up_enabled {
        links.push((with_return(format!("{base}/sign-up"), return_to), "Create an account"));
    }
    let status = if error.is_some() { StatusCode::UNAUTHORIZED } else { StatusCode::OK };
    Ok(Page { tenant: &tenant.name, title: "Sign in", error, fields: &[EMAIL, PASSWORD], submit: Some("Sign in"), socials, links, ..Default::default() }
        .respond(status))
}

async fn sign_in_form(State(st): State<AppState>, tenant: Tenant, Query(q): Query<ReturnTo>) -> Result<Response> {
    sign_in_page(&st, &tenant, &q, None).await
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

async fn sign_in(
    State(st): State<AppState>,
    tenant: Tenant,
    Query(q): Query<ReturnTo>,
    headers: HeaderMap,
    Form(f): Form<Credentials>,
) -> Result<Response> {
    // one message for unknown email, wrong password and suspended account
    let Some(user) = identity::authenticate(&st.db, &tenant, &f.email, f.password).await? else {
        return sign_in_page(&st, &tenant, &q, Some("Invalid email or password.")).await;
    };
    signed_in(&st, &tenant, user.id, &headers, q.safe(&tenant)).await
}

/// Starts the browser session, then continues the authorization request that sent the user here.
pub async fn signed_in(st: &AppState, tenant: &Tenant, user_id: Uuid, headers: &HeaderMap, return_to: Option<&str>) -> Result<Response> {
    let cookie = identity::create_session(&st.db, &st.cfg, tenant, user_id, headers).await?;
    let mut res = match return_to {
        Some(r) => Redirect::to(r).into_response(),
        None => ui(tenant, "", &[])
            .unwrap_or_else(|| message(tenant, "Signed in", Some("You are signed in. You can return to the application."), None, false)),
    };
    res.headers_mut().append(header::SET_COOKIE, cookie);
    Ok(res)
}

/// A page with no form. `back` adds a link to the sign-in page.
pub fn message(tenant: &Tenant, title: &str, notice: Option<&str>, error: Option<&str>, back: bool) -> Response {
    let links = if back { vec![(format!("{}/sign-in", tenant.base()), "Back to sign in")] } else { vec![] };
    let status = if error.is_some() { StatusCode::BAD_REQUEST } else { StatusCode::OK };
    Page { tenant: &tenant.name, title, notice, error, links, ..Default::default() }.respond(status)
}

/// `error` is (code for the login UI, text for the hosted page).
fn sign_up_page(tenant: &Tenant, q: &ReturnTo, error: Option<(&str, &str)>) -> Response {
    let code = if tenant.config.sign_up_enabled { error.map(|e| e.0) } else { Some("signup_disabled") };
    if let Some(r) = ui(tenant, "sign-up", &[("error", code), ("return_to", q.safe(tenant))]) {
        return r;
    }
    let error = error.map(|e| e.1);
    if !tenant.config.sign_up_enabled {
        return message(tenant, "Sign up", None, Some("Sign-up is disabled for this tenant."), true);
    }
    let links = vec![(with_return(format!("{}/sign-in", tenant.base()), q.safe(tenant)), "Already have an account? Sign in")];
    let status = if error.is_some() { StatusCode::BAD_REQUEST } else { StatusCode::OK };
    Page { tenant: &tenant.name, title: "Create account", error, fields: &[NAME, EMAIL, NEW_PASSWORD], submit: Some("Create account"), links, ..Default::default() }
        .respond(status)
}

async fn sign_up_form(tenant: Tenant, Query(q): Query<ReturnTo>) -> Response {
    sign_up_page(&tenant, &q, None)
}

#[derive(Deserialize)]
struct SignUp {
    name: String,
    email: String,
    password: String,
}

async fn sign_up(State(st): State<AppState>, tenant: Tenant, Query(q): Query<ReturnTo>, Form(f): Form<SignUp>) -> Result<Response> {
    if !tenant.config.sign_up_enabled {
        return Ok(sign_up_page(&tenant, &q, None));
    }
    let Some(email) = identity::normalize_email(&f.email) else {
        return Ok(sign_up_page(&tenant, &q, Some(("invalid_email", "Enter a valid email address."))));
    };
    if let Err(AppError::BadRequest(msg)) = identity::check_password(&tenant, &f.password) {
        return Ok(sign_up_page(&tenant, &q, Some(("weak_password", msg.as_str()))));
    }
    let hash = identity::hash_password(f.password).await?;
    let created: Option<Uuid> = sqlx::query_scalar(
        "insert into app_user (tenant_id, email, name, password_hash) values ($1, $2, $3, $4)
         on conflict (tenant_id, email) do nothing returning id",
    )
    .bind(tenant.id)
    .bind(&email)
    .bind(f.name.trim().chars().take(200).collect::<String>())
    .bind(hash)
    .fetch_optional(&st.db)
    .await?;
    if let Some(user_id) = created {
        let token = identity::create_verification(&st.db, tenant.id, user_id, "verify_email", 48).await?;
        let link = format!("{}/verify-email?token={token}", tenant.issuer(&st.cfg));
        let (cfg, subject) = (st.cfg.clone(), format!("Verify your email for {}", tenant.name));
        tokio::spawn(async move { mail::send(&cfg, &email, &subject, &format!("Confirm your email address:\n\n{link}\n")).await });
    }
    // same answer whether or not the address was already registered
    Ok(ui(&tenant, "sign-in", &[("notice", Some("check_email")), ("return_to", q.safe(&tenant))]).unwrap_or_else(|| {
        message(&tenant, "Check your email", Some("If that address is new, we sent a verification link. You can sign in now."), None, true)
    }))
}

#[derive(Deserialize)]
struct Token {
    token: Option<String>,
}

async fn verify_email(State(st): State<AppState>, tenant: Tenant, Query(q): Query<Token>) -> Result<Response> {
    let token = q.token.unwrap_or_default();
    let Some(user_id) = identity::consume_verification(&st.db, tenant.id, &token, "verify_email").await? else {
        return Ok(ui(&tenant, "sign-in", &[("error", Some("link_expired"))])
            .unwrap_or_else(|| message(&tenant, "Verify email", None, Some("This link is invalid or has expired."), true)));
    };
    sqlx::query("update app_user set email_verified = true where id = $1").bind(user_id).execute(&st.db).await?;
    Ok(ui(&tenant, "sign-in", &[("notice", Some("email_verified"))])
        .unwrap_or_else(|| message(&tenant, "Email verified", Some("Your email address is verified."), None, true)))
}

fn forgot_page(tenant: &Tenant, notice: Option<&str>) -> Response {
    if let Some(r) = ui(tenant, "forgot-password", &[("notice", notice.map(|_| "reset_sent"))]) {
        return r;
    }
    let links = vec![(format!("{}/sign-in", tenant.base()), "Back to sign in")];
    Page { tenant: &tenant.name, title: "Reset password", notice, fields: &[EMAIL], submit: Some("Send reset link"), links, ..Default::default() }
        .respond(StatusCode::OK)
}

async fn forgot_form(tenant: Tenant) -> Response {
    forgot_page(&tenant, None)
}

#[derive(Deserialize)]
struct Email {
    email: String,
}

async fn forgot(State(st): State<AppState>, tenant: Tenant, Form(f): Form<Email>) -> Result<Response> {
    if let Some(email) = identity::normalize_email(&f.email) {
        let user_id: Option<Uuid> =
            sqlx::query_scalar("select id from app_user where tenant_id = $1 and email = $2 and not suspended")
                .bind(tenant.id)
                .bind(&email)
                .fetch_optional(&st.db)
                .await?;
        if let Some(user_id) = user_id {
            let token = identity::create_verification(&st.db, tenant.id, user_id, "reset_password", 1).await?;
            let base = tenant.config.login_ui_url.clone().unwrap_or_else(|| tenant.issuer(&st.cfg));
            let link = format!("{base}/reset-password?token={token}");
            let (cfg, subject) = (st.cfg.clone(), format!("Reset your password for {}", tenant.name));
            // spawned so the response time does not reveal whether the account exists
            tokio::spawn(async move {
                mail::send(&cfg, &email, &subject, &format!("Reset your password (valid for one hour):\n\n{link}\n")).await
            });
        }
    }
    Ok(forgot_page(&tenant, Some("If an account exists for that address, a reset link is on its way.")))
}

fn reset_page(tenant: &Tenant, token: Option<&str>, error: Option<&str>) -> Response {
    if let Some(r) = ui(tenant, "reset-password", &[("token", token), ("error", error.map(|_| "weak_password"))]) {
        return r;
    }
    let status = if error.is_some() { StatusCode::BAD_REQUEST } else { StatusCode::OK };
    Page { tenant: &tenant.name, title: "Choose a new password", error, fields: &[NEW_PASSWORD], submit: Some("Update password"), ..Default::default() }
        .respond(status)
}

async fn reset_form(tenant: Tenant, Query(q): Query<Token>) -> Response {
    reset_page(&tenant, q.token.as_deref(), None)
}

#[derive(Deserialize)]
struct NewPassword {
    password: String,
}

async fn reset(State(st): State<AppState>, tenant: Tenant, Query(q): Query<Token>, Form(f): Form<NewPassword>) -> Result<Response> {
    // checked before the token is consumed, so a rejected password does not burn the link
    if let Err(AppError::BadRequest(msg)) = identity::check_password(&tenant, &f.password) {
        return Ok(reset_page(&tenant, q.token.as_deref(), Some(&msg)));
    }
    let token = q.token.unwrap_or_default();
    let Some(user_id) = identity::consume_verification(&st.db, tenant.id, &token, "reset_password").await? else {
        return Ok(ui(&tenant, "forgot-password", &[("error", Some("link_expired"))])
            .unwrap_or_else(|| message(&tenant, "Reset password", None, Some("This link is invalid or has expired."), true)));
    };
    let hash = identity::hash_password(f.password).await?;
    // the link proved control of the mailbox, so the address counts as verified
    sqlx::query("update app_user set password_hash = $1, email_verified = true where id = $2")
        .bind(hash)
        .bind(user_id)
        .execute(&st.db)
        .await?;
    identity::revoke_user(&st.db, user_id).await?;
    Ok(ui(&tenant, "sign-in", &[("notice", Some("password_updated"))]).unwrap_or_else(|| {
        message(&tenant, "Password updated", Some("Your password was changed and all other sessions were signed out."), None, true)
    }))
}
