//! Hosted sign-in experience: server-rendered forms, same origin as the session cookie.

use askama::Template;
use axum::{
    extract::{Query, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Form, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::{internal, AppError, Result},
    identity, social,
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

/// CSRF defence for the cookie-authenticated forms: a POST must carry our own Origin, which browsers
/// always send on form posts and pages on other sites cannot forge. Also sets the page security headers.
async fn guard(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if req.method() == Method::POST && origin != Some(st.cfg.public_url.as_str()) {
        return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
    }
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
    let cookie = identity::create_session(&st.db, &st.cfg, tenant, identity::Surface::Hosted, user_id, headers).await?;
    let mut res = match return_to {
        Some(r) => Redirect::to(r).into_response(),
        None => message(tenant, "Signed in", Some("You are signed in. You can return to the application."), None, false),
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

fn sign_up_page(tenant: &Tenant, q: &ReturnTo, error: Option<&str>) -> Response {
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
    match identity::register(&st, &tenant, &f.name, &f.email, f.password).await {
        Err(AppError::BadRequest(msg)) => return Ok(sign_up_page(&tenant, &q, Some(&msg))),
        r => r?,
    }
    // same answer whether or not the address was already registered
    Ok(message(&tenant, "Check your email", Some("If that address is new, we sent a verification link. You can sign in now."), None, true))
}

#[derive(Deserialize)]
struct Token {
    token: Option<String>,
}

async fn verify_email(State(st): State<AppState>, tenant: Tenant, Query(q): Query<Token>) -> Result<Response> {
    if !identity::verify_email(&st, &tenant, &q.token.unwrap_or_default()).await? {
        return Ok(message(&tenant, "Verify email", None, Some("This link is invalid or has expired."), true));
    }
    Ok(message(&tenant, "Email verified", Some("Your email address is verified."), None, true))
}

fn forgot_page(tenant: &Tenant, notice: Option<&str>) -> Response {
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
    identity::send_reset(&st, &tenant, &f.email).await?;
    Ok(forgot_page(&tenant, Some("If an account exists for that address, a reset link is on its way.")))
}

fn reset_page(tenant: &Tenant, error: Option<&str>) -> Response {
    let status = if error.is_some() { StatusCode::BAD_REQUEST } else { StatusCode::OK };
    Page { tenant: &tenant.name, title: "Choose a new password", error, fields: &[NEW_PASSWORD], submit: Some("Update password"), ..Default::default() }
        .respond(status)
}

async fn reset_form(tenant: Tenant) -> Response {
    reset_page(&tenant, None)
}

#[derive(Deserialize)]
struct NewPassword {
    password: String,
}

async fn reset(State(st): State<AppState>, tenant: Tenant, Query(q): Query<Token>, Form(f): Form<NewPassword>) -> Result<Response> {
    match identity::reset_password(&st, &tenant, &q.token.unwrap_or_default(), f.password).await {
        Err(AppError::BadRequest(msg)) => return Ok(reset_page(&tenant, Some(&msg))),
        Ok(false) => return Ok(message(&tenant, "Reset password", None, Some("This link is invalid or has expired."), true)),
        r => r?,
    };
    Ok(message(&tenant, "Password updated", Some("Your password was changed and all other sessions were signed out."), None, true))
}
