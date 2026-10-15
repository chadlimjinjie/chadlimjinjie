//! OpenID Connect provider endpoints: authorization code + PKCE, refresh tokens, userinfo, logout.

use axum::{
    extract::{Query, RawQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

use crate::{
    error::{bad, internal, AppError, Result},
    identity::{self, random_token, sha256, User},
    keys, pages,
    tenant::Tenant,
    AppState,
};

/// Scopes any user may be granted; everything else must come from one of the user's roles.
pub const OIDC_SCOPES: [&str; 5] = ["openid", "profile", "email", "offline_access", "organizations"];

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Client {
    pub id: Uuid,
    pub client_id: String,
    pub name: String,
    #[serde(skip)]
    pub secret_hash: Option<Vec<u8>>,
    pub redirect_uris: Vec<String>,
    pub post_logout_uris: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub confidential: bool,
}

pub const CLIENT_COLS: &str =
    "id, client_id, name, secret_hash, redirect_uris, post_logout_uris, created_at, secret_hash is not null as confidential";

async fn find_client(db: &PgPool, tenant_id: Uuid, client_id: &str) -> Result<Option<Client>> {
    Ok(sqlx::query_as(&format!("select {CLIENT_COLS} from oidc_client where tenant_id = $1 and client_id = $2"))
        .bind(tenant_id)
        .bind(client_id)
        .fetch_optional(db)
        .await?)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/userinfo", get(userinfo).post(userinfo))
        .route("/end-session", get(end_session))
}

async fn discovery(State(st): State<AppState>, tenant: Tenant) -> Json<Value> {
    let iss = tenant.issuer(&st.cfg);
    Json(json!({
        "issuer": iss,
        "authorization_endpoint": format!("{iss}/authorize"),
        "token_endpoint": format!("{iss}/token"),
        "userinfo_endpoint": format!("{iss}/userinfo"),
        "jwks_uri": format!("{iss}/jwks"),
        "end_session_endpoint": format!("{iss}/end-session"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": OIDC_SCOPES,
        "token_endpoint_auth_methods_supported": ["none", "client_secret_basic", "client_secret_post"],
        "code_challenge_methods_supported": ["S256"],
        "claims_supported": ["sub", "iss", "aud", "exp", "iat", "auth_time", "nonce", "email", "email_verified", "name", "organizations"],
    }))
}

async fn jwks(State(st): State<AppState>, tenant: Tenant) -> Result<Json<Value>> {
    Ok(Json(keys::jwks(&st.db, tenant.id).await?))
}

#[derive(Deserialize)]
struct AuthorizeParams {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    nonce: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    prompt: Option<String>,
}

async fn authorize(
    State(st): State<AppState>,
    tenant: Tenant,
    Query(p): Query<AuthorizeParams>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response> {
    // Until client and redirect_uri are proven, errors are shown here and never redirected.
    let client = find_client(&st.db, tenant.id, p.client_id.as_deref().unwrap_or_default())
        .await?
        .ok_or_else(|| bad("unknown client_id"))?;
    let redirect_uri = p
        .redirect_uri
        .as_deref()
        .filter(|uri| client.redirect_uris.iter().any(|registered| registered == uri))
        .ok_or_else(|| bad("redirect_uri is not registered for this client"))?;
    let target = Url::parse(redirect_uri).map_err(|_| bad("redirect_uri is not a valid URL"))?;

    let back = |pairs: &[(&str, &str)]| {
        let mut url = target.clone();
        {
            let mut q = url.query_pairs_mut();
            for (k, v) in pairs {
                q.append_pair(k, v);
            }
            if let Some(state) = &p.state {
                q.append_pair("state", state);
            }
        }
        Redirect::to(url.as_str()).into_response()
    };
    let fail = |error: &str, description: &str| back(&[("error", error), ("error_description", description)]);

    if p.response_type.as_deref() != Some("code") {
        return Ok(fail("unsupported_response_type", "only response_type=code is supported"));
    }
    let scope = p.scope.as_deref().unwrap_or_default();
    if !scope.split_whitespace().any(|s| s == "openid") {
        return Ok(fail("invalid_scope", "scope must include openid"));
    }
    let challenge = match (&p.code_challenge, p.code_challenge_method.as_deref()) {
        (Some(c), Some("S256")) if (43..=128).contains(&c.len()) => c,
        _ => return Ok(fail("invalid_request", "PKCE with code_challenge_method=S256 is required")),
    };

    let Some((user, auth_time)) = identity::current_user(&st.db, &tenant, &headers).await? else {
        if p.prompt.as_deref() == Some("none") {
            return Ok(fail("login_required", "no active session"));
        }
        let return_to = format!("{}/authorize?{}", tenant.base(), raw.unwrap_or_default());
        return Ok(Redirect::to(&pages::with_return(format!("{}/sign-in", tenant.base()), Some(&return_to))).into_response());
    };

    // All clients are first-party, so there is no consent step.
    let code = random_token();
    sqlx::query(
        "insert into auth_code (code_hash, tenant_id, client_id, user_id, redirect_uri, scope, code_challenge, nonce, auth_time, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, now() + interval '60 seconds')",
    )
    .bind(sha256(&code))
    .bind(tenant.id)
    .bind(client.id)
    .bind(user.id)
    .bind(redirect_uri)
    .bind(scope)
    .bind(challenge)
    .bind(&p.nonce)
    .bind(auth_time)
    .execute(&st.db)
    .await?;
    Ok(back(&[("code", &code)]))
}

/// RFC 6749 §5.2 error response.
struct OAuthError(StatusCode, &'static str, &'static str);

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let body = Json(json!({ "error": self.1, "error_description": self.2 }));
        (self.0, [(header::CACHE_CONTROL, "no-store")], body).into_response()
    }
}

impl From<AppError> for OAuthError {
    fn from(_: AppError) -> Self {
        OAuthError(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "internal error")
    }
}

impl From<sqlx::Error> for OAuthError {
    fn from(e: sqlx::Error) -> Self {
        internal(e).into()
    }
}

fn invalid_grant(description: &'static str) -> OAuthError {
    OAuthError(StatusCode::BAD_REQUEST, "invalid_grant", description)
}

fn invalid_client() -> OAuthError {
    OAuthError(StatusCode::UNAUTHORIZED, "invalid_client", "client authentication failed")
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AuthCode {
    client_id: Uuid,
    user_id: Uuid,
    redirect_uri: String,
    scope: String,
    code_challenge: String,
    nonce: Option<String>,
    family_id: Uuid,
    auth_time: DateTime<Utc>,
}

async fn token(
    State(st): State<AppState>,
    tenant: Tenant,
    headers: HeaderMap,
    Form(f): Form<TokenForm>,
) -> std::result::Result<Response, OAuthError> {
    // client_secret_basic, else client_secret_post, else a public client identified by client_id alone
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b| STANDARD.decode(b).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.split_once(':').map(|(id, secret)| (id.to_string(), secret.to_string())));
    let (client_id, secret) = match basic {
        Some((id, secret)) => (id, Some(secret)),
        None => (f.client_id.clone().ok_or_else(invalid_client)?, f.client_secret.clone()),
    };
    let client = find_client(&st.db, tenant.id, &client_id).await?.ok_or_else(invalid_client)?;
    if let Some(hash) = &client.secret_hash {
        let presented = sha256(secret.as_deref().unwrap_or_default());
        if !bool::from(presented.as_slice().ct_eq(hash.as_slice())) {
            return Err(invalid_client());
        }
    }

    let grant = match f.grant_type.as_str() {
        "authorization_code" => {
            let code_hash = sha256(f.code.as_deref().unwrap_or_default());
            let code: Option<AuthCode> = sqlx::query_as(
                "update auth_code set used_at = now()
                 where code_hash = $1 and tenant_id = $2 and used_at is null and expires_at > now()
                 returning client_id, user_id, redirect_uri, scope, code_challenge, nonce, family_id, auth_time",
            )
            .bind(&code_hash)
            .bind(tenant.id)
            .fetch_optional(&st.db)
            .await?;
            let Some(code) = code else {
                // A code presented twice means it leaked: revoke the refresh tokens it produced.
                sqlx::query(
                    "update refresh_token set revoked_at = now() where revoked_at is null and tenant_id = $2
                     and family_id = (select family_id from auth_code where code_hash = $1 and tenant_id = $2)",
                )
                .bind(&code_hash)
                .bind(tenant.id)
                .execute(&st.db)
                .await?;
                return Err(invalid_grant("authorization code is invalid, expired or already used"));
            };
            let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(f.code_verifier.as_deref().unwrap_or_default()));
            let pkce_ok = bool::from(challenge.as_bytes().ct_eq(code.code_challenge.as_bytes()));
            if code.client_id != client.id || f.redirect_uri.as_deref() != Some(code.redirect_uri.as_str()) || !pkce_ok {
                return Err(invalid_grant("code does not match client, redirect_uri or code_verifier"));
            }
            Grant { user_id: code.user_id, scope: code.scope, family_id: code.family_id, nonce: code.nonce, auth_time: Some(code.auth_time) }
        }
        "refresh_token" => {
            let token_hash = sha256(f.refresh_token.as_deref().unwrap_or_default());
            // Rotation: each refresh token works once.
            let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
                "update refresh_token set revoked_at = now()
                 where token_hash = $1 and tenant_id = $2 and client_id = $3 and revoked_at is null and expires_at > now()
                 returning user_id, family_id, scope",
            )
            .bind(&token_hash)
            .bind(tenant.id)
            .bind(client.id)
            .fetch_optional(&st.db)
            .await?;
            let Some((user_id, family_id, scope)) = row else {
                // Reuse of a rotated token means it was stolen: revoke every token descended from it.
                sqlx::query(
                    "update refresh_token set revoked_at = now() where revoked_at is null and tenant_id = $2
                     and family_id = (select family_id from refresh_token where token_hash = $1 and tenant_id = $2)",
                )
                .bind(&token_hash)
                .bind(tenant.id)
                .execute(&st.db)
                .await?;
                return Err(invalid_grant("refresh token is invalid, expired or already used"));
            };
            Grant { user_id, scope, family_id, nonce: None, auth_time: None }
        }
        _ => return Err(OAuthError(StatusCode::BAD_REQUEST, "unsupported_grant_type", "use authorization_code or refresh_token")),
    };

    let user: Option<User> = sqlx::query_as("select * from app_user where id = $1 and tenant_id = $2 and not suspended")
        .bind(grant.user_id)
        .bind(tenant.id)
        .fetch_optional(&st.db)
        .await?;
    let user = user.ok_or_else(|| invalid_grant("user is no longer active"))?;
    let body = issue_tokens(&st, &tenant, &client, &user, grant).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response())
}

struct Grant {
    user_id: Uuid,
    scope: String,
    family_id: Uuid,
    nonce: Option<String>,
    auth_time: Option<DateTime<Utc>>,
}

async fn issue_tokens(st: &AppState, tenant: &Tenant, client: &Client, user: &User, grant: Grant) -> Result<Value> {
    // Recomputed on every issue, so a role change takes effect at the next refresh.
    let role_scopes: Vec<String> = sqlx::query_scalar(
        "select distinct unnest(r.scopes) from role r join user_role ur on ur.role_id = r.id
         where ur.user_id = $1 and r.tenant_id = $2",
    )
    .bind(user.id)
    .bind(tenant.id)
    .fetch_all(&st.db)
    .await?;
    let mut scopes: Vec<&str> = Vec::new();
    for s in grant.scope.split_whitespace() {
        if !scopes.contains(&s) && (OIDC_SCOPES.contains(&s) || role_scopes.iter().any(|r| r == s)) {
            scopes.push(s);
        }
    }
    let scope = scopes.join(" ");

    let issuer = tenant.issuer(&st.cfg);
    let now = Utc::now().timestamp();
    let ttl = tenant.config.access_token_ttl_secs;

    let mut access = Map::new();
    access.insert("iss".into(), json!(issuer));
    access.insert("sub".into(), json!(user.id));
    access.insert("aud".into(), json!(issuer));
    access.insert("client_id".into(), json!(client.client_id));
    access.insert("tenant".into(), json!(tenant.slug));
    access.insert("scope".into(), json!(scope));
    access.insert("iat".into(), json!(now));
    access.insert("exp".into(), json!(now + ttl));
    if scopes.contains(&"organizations") {
        access.insert("organizations".into(), organizations(&st.db, user.id).await?);
    }

    let mut id = profile_claims(user, &scopes);
    id.insert("iss".into(), json!(issuer));
    id.insert("sub".into(), json!(user.id));
    id.insert("aud".into(), json!(client.client_id));
    id.insert("iat".into(), json!(now));
    id.insert("exp".into(), json!(now + ttl));
    if let Some(auth_time) = grant.auth_time {
        id.insert("auth_time".into(), json!(auth_time.timestamp()));
    }
    if let Some(nonce) = &grant.nonce {
        id.insert("nonce".into(), json!(nonce));
    }

    let mut body = json!({
        "access_token": keys::sign(&st.db, tenant.id, &Value::Object(access)).await?,
        "id_token": keys::sign(&st.db, tenant.id, &Value::Object(id)).await?,
        "token_type": "Bearer",
        "expires_in": ttl,
        "scope": scope,
    });
    if scopes.contains(&"offline_access") {
        let refresh = random_token();
        sqlx::query(
            "insert into refresh_token (token_hash, tenant_id, family_id, client_id, user_id, scope, expires_at)
             values ($1, $2, $3, $4, $5, $6, now() + make_interval(days => $7))",
        )
        .bind(sha256(&refresh))
        .bind(tenant.id)
        .bind(grant.family_id)
        .bind(client.id)
        .bind(user.id)
        // the requested scope is kept, so scopes from a role granted later appear on refresh
        .bind(&grant.scope)
        .bind(tenant.config.refresh_token_ttl_days as i32)
        .execute(&st.db)
        .await?;
        body["refresh_token"] = json!(refresh);
    }
    Ok(body)
}

fn profile_claims(user: &User, scopes: &[&str]) -> Map<String, Value> {
    let mut claims = Map::new();
    if scopes.contains(&"email") {
        claims.insert("email".into(), json!(user.email));
        claims.insert("email_verified".into(), json!(user.email_verified));
    }
    if scopes.contains(&"profile") {
        claims.insert("name".into(), json!(user.name));
    }
    claims
}

async fn organizations(db: &PgPool, user_id: Uuid) -> Result<Value> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "select o.id, o.name, m.role from org_member m join organization o on o.id = m.org_id
         where m.user_id = $1 order by o.name",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(id, name, role)| json!({ "id": id, "name": name, "role": role })).collect())
}

pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")
}

async fn userinfo(State(st): State<AppState>, tenant: Tenant, headers: HeaderMap) -> Result<Response> {
    let unauthorized = || (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")]).into_response();
    let Some(token) = bearer(&headers) else { return Ok(unauthorized()) };
    let Some(claims) = keys::verify_access(&st, &tenant, token).await else { return Ok(unauthorized()) };
    let user: Option<User> = sqlx::query_as("select * from app_user where id = $1 and tenant_id = $2 and not suspended")
        .bind(claims.sub)
        .bind(tenant.id)
        .fetch_optional(&st.db)
        .await?;
    let Some(user) = user else { return Ok(unauthorized()) };
    let scopes: Vec<&str> = claims.scope.split_whitespace().collect();
    let mut body = profile_claims(&user, &scopes);
    body.insert("sub".into(), json!(user.id));
    if scopes.contains(&"organizations") {
        body.insert("organizations".into(), organizations(&st.db, user.id).await?);
    }
    Ok(Json(body).into_response())
}

#[derive(Deserialize)]
struct EndSessionParams {
    id_token_hint: Option<String>,
    client_id: Option<String>,
    post_logout_redirect_uri: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
struct HintClaims {
    aud: String,
}

/// RP-initiated logout: ends the browser session and the client's refresh tokens for this user.
async fn end_session(
    State(st): State<AppState>,
    tenant: Tenant,
    Query(p): Query<EndSessionParams>,
    headers: HeaderMap,
) -> Result<Response> {
    let mut client_id = p.client_id.clone();
    if let (None, Some(hint)) = (&client_id, &p.id_token_hint) {
        // an expired ID token is still a valid hint
        let mut v = Validation::new(Algorithm::RS256);
        v.validate_exp = false;
        v.validate_aud = false;
        v.set_issuer(&[tenant.issuer(&st.cfg)]);
        client_id = keys::verify::<HintClaims>(&st.db, tenant.id, hint, &v).await.map(|c| c.aud);
    }
    let client = match &client_id {
        Some(id) => find_client(&st.db, tenant.id, id).await?,
        None => None,
    };

    if let (Some((user, _)), Some(client)) = (identity::current_user(&st.db, &tenant, &headers).await?, &client) {
        sqlx::query("update refresh_token set revoked_at = now() where user_id = $1 and client_id = $2 and revoked_at is null")
            .bind(user.id)
            .bind(client.id)
            .execute(&st.db)
            .await?;
    }
    let cookie = identity::end_session(&st.db, &st.cfg, &tenant, &headers).await?;

    // Only a URI registered on the identified client is followed.
    let target = p
        .post_logout_redirect_uri
        .as_deref()
        .filter(|uri| client.as_ref().is_some_and(|c| c.post_logout_uris.iter().any(|registered| registered == uri)))
        .and_then(|uri| Url::parse(uri).ok());
    let mut res = match target {
        Some(mut url) => {
            if let Some(state) = &p.state {
                url.query_pairs_mut().append_pair("state", state);
            }
            Redirect::to(url.as_str()).into_response()
        }
        None => pages::message(&tenant, "Signed out", Some("You have been signed out."), None, true),
    };
    res.headers_mut().append(header::SET_COOKIE, cookie);
    Ok(res)
}
