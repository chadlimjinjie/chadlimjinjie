//! Social sign-in (Google, GitHub) as an OAuth client, feeding the same browser session as passwords.

use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    identity::{self, random_token, User},
    pages::{self, ReturnTo},
    tenant::Tenant,
    AppState,
};

const STATE_COOKIE: &str = "auth_social";

struct Provider {
    authorize: &'static str,
    token: &'static str,
    scope: &'static str,
}

fn provider(name: &str) -> Option<Provider> {
    match name {
        "google" => Some(Provider {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth",
            token: "https://oauth2.googleapis.com/token",
            scope: "openid email profile",
        }),
        "github" => Some(Provider {
            authorize: "https://github.com/login/oauth/authorize",
            token: "https://github.com/login/oauth/access_token",
            scope: "read:user user:email",
        }),
        _ => None,
    }
}

pub fn label(provider: &str) -> &'static str {
    match provider {
        "google" => "Google",
        "github" => "GitHub",
        _ => "provider",
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/social/{provider}", get(start)).route("/social/{provider}/callback", get(callback))
}

#[derive(Deserialize)]
struct ProviderPath {
    provider: String,
}

async fn connector(db: &PgPool, tenant_id: Uuid, provider: &str) -> Result<(String, String)> {
    sqlx::query_as("select client_id, client_secret from social_connector where tenant_id = $1 and provider = $2 and enabled")
        .bind(tenant_id)
        .bind(provider)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound)
}

fn callback_url(st: &AppState, tenant: &Tenant, provider: &str) -> String {
    format!("{}/social/{provider}/callback", tenant.issuer(&st.cfg))
}

async fn start(
    State(st): State<AppState>,
    tenant: Tenant,
    Path(path): Path<ProviderPath>,
    Query(q): Query<ReturnTo>,
) -> Result<Response> {
    let p = provider(&path.provider).ok_or(AppError::NotFound)?;
    let (client_id, _) = connector(&st.db, tenant.id, &path.provider).await?;
    // The state carries return_to; the cookie holds the same string, so the callback can trust both.
    let state = format!("{}.{}", random_token(), URL_SAFE_NO_PAD.encode(q.safe(&tenant).unwrap_or_default()));
    let url = Url::parse_with_params(
        p.authorize,
        [
            ("client_id", client_id.as_str()),
            ("redirect_uri", &callback_url(&st, &tenant, &path.provider)),
            ("response_type", "code"),
            ("scope", p.scope),
            ("state", &state),
        ],
    )
    .map_err(crate::error::internal)?;
    let cookie = identity::set_cookie(&st.cfg, &tenant, STATE_COOKIE, &state, 600);
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to(url.as_str())).into_response())
}

#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
}

/// What a provider vouches for. `email` is only set when the provider says it is verified.
pub struct Profile {
    pub id: String,
    pub email: Option<String>,
    pub name: String,
}

async fn callback(
    State(st): State<AppState>,
    tenant: Tenant,
    Path(path): Path<ProviderPath>,
    Query(q): Query<Callback>,
    headers: HeaderMap,
) -> Result<Response> {
    let failed = |why: &str| Ok(pages::message(&tenant, "Sign in", None, Some(why), true));
    let p = provider(&path.provider).ok_or(AppError::NotFound)?;
    let (client_id, client_secret) = connector(&st.db, tenant.id, &path.provider).await?;

    let (Some(code), Some(state)) = (q.code, q.state) else { return failed("Sign-in was cancelled.") };
    if identity::cookie(&headers, STATE_COOKIE) != Some(state.as_str()) {
        return failed("Sign-in session expired. Please try again.");
    }

    let profile = match fetch_profile(&path.provider, &p, &client_id, &client_secret, &code, &callback_url(&st, &tenant, &path.provider)).await {
        Ok(profile) => profile,
        Err(e) => {
            tracing::warn!("{} sign-in failed: {e}", path.provider);
            return failed("The provider did not complete the sign-in.");
        }
    };
    let user = match link_or_create(&st.db, &tenant, &path.provider, profile).await? {
        Ok(user) => user,
        Err(why) => return failed(why),
    };

    let return_to = state
        .split_once('.')
        .and_then(|(_, r)| URL_SAFE_NO_PAD.decode(r).ok())
        .and_then(|r| String::from_utf8(r).ok())
        .filter(|r| !r.is_empty());
    let return_to = ReturnTo { return_to };
    let mut res = pages::signed_in(&st, &tenant, user.id, &headers, return_to.safe(&tenant)).await?;
    res.headers_mut().append(header::SET_COOKIE, identity::set_cookie(&st.cfg, &tenant, STATE_COOKIE, "", 0));
    Ok(res)
}

async fn fetch_profile(
    name: &str,
    p: &Provider,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> std::result::Result<Profile, reqwest::Error> {
    // GitHub's API rejects requests without a User-Agent
    let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).user_agent("auth-service").build()?;
    let token: Value = http
        .post(p.token)
        .header(header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let access = token["access_token"].as_str().unwrap_or_default();
    let get = |url: &'static str| http.get(url).bearer_auth(access).send();

    if name == "google" {
        let u: Value = get("https://openidconnect.googleapis.com/v1/userinfo").await?.error_for_status()?.json().await?;
        let verified = u["email_verified"].as_bool().unwrap_or(false);
        return Ok(Profile {
            id: u["sub"].as_str().unwrap_or_default().to_string(),
            email: u["email"].as_str().filter(|_| verified).map(str::to_string),
            name: u["name"].as_str().unwrap_or_default().to_string(),
        });
    }
    let u: Value = get("https://api.github.com/user").await?.error_for_status()?.json().await?;
    let emails: Value = get("https://api.github.com/user/emails").await?.error_for_status()?.json().await?;
    let email = emails.as_array().and_then(|list| {
        list.iter()
            .find(|e| e["primary"].as_bool() == Some(true) && e["verified"].as_bool() == Some(true))
            .and_then(|e| e["email"].as_str().map(str::to_string))
    });
    Ok(Profile {
        id: u["id"].as_i64().map(|id| id.to_string()).unwrap_or_default(),
        email,
        name: u["name"].as_str().or(u["login"].as_str()).unwrap_or_default().to_string(),
    })
}

/// Maps a provider identity to a local user. The inner error is a message for the person signing in.
pub async fn link_or_create(
    db: &PgPool,
    tenant: &Tenant,
    provider: &str,
    profile: Profile,
) -> Result<std::result::Result<User, &'static str>> {
    if profile.id.is_empty() {
        return Ok(Err("The provider did not return an account."));
    }
    let linked: Option<User> = sqlx::query_as(
        "select u.* from account a join app_user u on u.id = a.user_id
         where a.tenant_id = $1 and a.provider = $2 and a.provider_account_id = $3",
    )
    .bind(tenant.id)
    .bind(provider)
    .bind(&profile.id)
    .fetch_optional(db)
    .await?;

    let user = match linked {
        Some(user) => user,
        None => {
            // Accounts are matched by email, so an address the provider has not verified links nothing.
            let Some(email) = profile.email.as_deref().and_then(identity::normalize_email) else {
                return Ok(Err("Your account with this provider has no verified email address."));
            };
            let existing: Option<User> = sqlx::query_as("select * from app_user where tenant_id = $1 and email = $2")
                .bind(tenant.id)
                .bind(&email)
                .fetch_optional(db)
                .await?;
            let user: User = match existing {
                Some(user) if user.email_verified => user,
                Some(user) => {
                    // Whoever registered this address never proved they own it. Drop their password and
                    // sessions, so they cannot share the account with the owner the provider just vouched for.
                    identity::revoke_user(db, user.id).await?;
                    sqlx::query_as("update app_user set password_hash = null, email_verified = true where id = $1 returning *")
                        .bind(user.id)
                        .fetch_one(db)
                        .await?
                }
                None if tenant.config.sign_up_enabled => {
                    sqlx::query_as("insert into app_user (tenant_id, email, name, email_verified) values ($1, $2, $3, true) returning *")
                        .bind(tenant.id)
                        .bind(&email)
                        .bind(profile.name.chars().take(200).collect::<String>())
                        .fetch_one(db)
                        .await?
                }
                None => return Ok(Err("No account exists for this email address.")),
            };
            sqlx::query("insert into account (tenant_id, user_id, provider, provider_account_id) values ($1, $2, $3, $4)")
                .bind(tenant.id)
                .bind(user.id)
                .bind(provider)
                .bind(&profile.id)
                .execute(db)
                .await?;
            user
        }
    };
    if user.suspended {
        return Ok(Err("This account is suspended."));
    }
    Ok(Ok(user))
}
