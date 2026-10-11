//! End-to-end tests against a real Postgres (`DATABASE_URL`), one fresh database per test.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use url::Url;
use uuid::Uuid;

use crate::{
    app, bootstrap, identity,
    social::{link_or_create, Profile},
    tenant::{self, Tenant, TenantConfig},
    AppState, Config,
};

const ORIGIN: &str = "http://auth.test";
const REDIRECT: &str = "https://app.test/cb";
const VERIFIER: &str = "a-code-verifier-that-is-at-least-43-characters-long";
const PASSWORD: &str = "correct horse battery";

fn state(db: PgPool) -> AppState {
    let cfg = Config {
        public_url: ORIGIN.into(),
        console_url: "https://console.test".into(),
        cors_origins: vec!["https://console.test".into()],
        smtp_url: None,
        mail_from: "auth@test".into(),
    };
    AppState { db, cfg: Arc::new(cfg) }
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let res = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    (parts.status, parts.headers, String::from_utf8_lossy(&bytes).into_owned())
}

fn form(uri: &str, body: &str) -> Request<Body> {
    Request::post(uri)
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn api(method: &str, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn location(headers: &HeaderMap) -> String {
    headers[header::LOCATION].to_str().unwrap().to_string()
}

fn param(url: &str, name: &str) -> Option<String> {
    Url::parse(url).unwrap().query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned())
}

async fn mk_tenant(st: &AppState, slug: &str) -> Tenant {
    let mut tx = st.db.begin().await.unwrap();
    let t = tenant::create(&mut tx, slug, slug, TenantConfig::default()).await.unwrap();
    sqlx::query("insert into oidc_client (tenant_id, client_id, name, redirect_uris, post_logout_uris) values ($1, 'app', 'App', $2, $3)")
        .bind(t.id)
        .bind(vec![REDIRECT])
        .bind(vec!["https://app.test/"])
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    t
}

async fn mk_user(st: &AppState, tenant: &Tenant, email: &str) -> Uuid {
    let hash = identity::hash_password(PASSWORD.into()).await.unwrap();
    sqlx::query_scalar("insert into app_user (tenant_id, email, password_hash, email_verified) values ($1, $2, $3, true) returning id")
        .bind(tenant.id)
        .bind(email)
        .bind(hash)
        .fetch_one(&st.db)
        .await
        .unwrap()
}

/// Signs in through the hosted form and returns the session cookie pair.
async fn login(app: &Router, slug: &str, email: &str) -> Option<String> {
    let (status, headers, _) = send(app, form(&format!("/auth/t/{slug}/sign-in"), &format!("email={email}&password={PASSWORD}"))).await;
    if status != StatusCode::OK {
        return None;
    }
    let cookie = headers[header::SET_COOKIE].to_str().unwrap();
    Some(cookie.split(';').next().unwrap().to_string())
}

fn authorize_url(slug: &str, client: &str, redirect: &str, scope: &str) -> String {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER));
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("response_type", "code")
        .append_pair("client_id", client)
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", scope)
        .append_pair("state", "xyz")
        .append_pair("nonce", "n-1")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .finish();
    format!("/auth/t/{slug}/authorize?{query}")
}

async fn get_code(app: &Router, slug: &str, cookie: &str, client: &str, redirect: &str, scope: &str) -> String {
    let req = Request::get(authorize_url(slug, client, redirect, scope)).header(header::COOKIE, cookie).body(Body::empty()).unwrap();
    let (status, headers, body) = send(app, req).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let to = location(&headers);
    assert!(to.starts_with(redirect), "{to}");
    assert_eq!(param(&to, "state").as_deref(), Some("xyz"));
    param(&to, "code").unwrap_or_else(|| panic!("no code in {to}"))
}

async fn exchange(app: &Router, slug: &str, body: &str) -> (StatusCode, Value) {
    let (status, _, body) = send(app, form(&format!("/auth/t/{slug}/token"), body)).await;
    (status, serde_json::from_str(&body).unwrap())
}

fn code_grant(code: &str, verifier: &str) -> String {
    format!("grant_type=authorization_code&client_id=app&code={code}&code_verifier={verifier}&redirect_uri={}", urlencode(REDIRECT))
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

async fn userinfo(app: &Router, slug: &str, token: &str) -> (StatusCode, String) {
    let req = Request::get(format!("/auth/t/{slug}/userinfo")).header(header::AUTHORIZATION, format!("Bearer {token}")).body(Body::empty()).unwrap();
    let (status, _, body) = send(app, req).await;
    (status, body)
}

/// Full sign-in + code exchange for the `app` client.
async fn tokens(app: &Router, slug: &str, email: &str, scope: &str) -> Value {
    let cookie = login(app, slug, email).await.expect("login");
    let code = get_code(app, slug, &cookie, "app", REDIRECT, scope).await;
    let (status, body) = exchange(app, slug, &code_grant(&code, VERIFIER)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[sqlx::test]
async fn code_flow_issues_tokens_that_verify_against_jwks(db: PgPool) {
    let st = state(db);
    let t = mk_tenant(&st, "acme").await;
    let user_id = mk_user(&st, &t, "ann@acme.test").await;
    let app = app(st);

    // signed out: authorize sends the browser to the hosted sign-in page and back
    let (status, headers, _) = send(&app, Request::get(authorize_url("acme", "app", REDIRECT, "openid")).body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let sign_in = location(&headers);
    assert!(sign_in.starts_with("/auth/t/acme/sign-in?return_to="), "{sign_in}");
    let (status, headers, _) = send(&app, form(&sign_in, &format!("email=ann@acme.test&password={PASSWORD}"))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).starts_with("/auth/t/acme/authorize?"));

    let body = tokens(&app, "acme", "ann@acme.test", "openid email profile offline_access manage:all").await;
    // manage:all was requested but no role grants it
    assert_eq!(body["scope"], "openid email profile offline_access");
    assert!(body["refresh_token"].is_string());

    let (_, _, jwks) = send(&app, Request::get("/auth/t/acme/jwks").body(Body::empty()).unwrap()).await;
    let jwks: Value = serde_json::from_str(&jwks).unwrap();
    let id_token = body["id_token"].as_str().unwrap();
    let kid = decode_header(id_token).unwrap().kid.unwrap();
    let jwk = jwks["keys"].as_array().unwrap().iter().find(|k| k["kid"] == kid.as_str()).expect("kid published");
    let key = DecodingKey::from_rsa_components(jwk["n"].as_str().unwrap(), jwk["e"].as_str().unwrap()).unwrap();
    let mut v = Validation::new(Algorithm::RS256);
    v.set_issuer(&["http://auth.test/auth/t/acme"]);
    v.set_audience(&["app"]);
    let claims = decode::<Value>(id_token, &key, &v).unwrap().claims;
    assert_eq!(claims["sub"], user_id.to_string());
    assert_eq!(claims["nonce"], "n-1");
    assert_eq!(claims["email"], "ann@acme.test");

    let (status, info) = userinfo(&app, "acme", body["access_token"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(info.contains("ann@acme.test"));
    // an ID token is not an access token
    assert_eq!(userinfo(&app, "acme", id_token).await.0, StatusCode::UNAUTHORIZED);

    let (_, _, discovery) = send(&app, Request::get("/auth/t/acme/.well-known/openid-configuration").body(Body::empty()).unwrap()).await;
    assert_eq!(serde_json::from_str::<Value>(&discovery).unwrap()["issuer"], "http://auth.test/auth/t/acme");
}

#[sqlx::test]
async fn code_exchange_rejects_bad_pkce_redirect_and_replay(db: PgPool) {
    let st = state(db);
    let t = mk_tenant(&st, "acme").await;
    mk_user(&st, &t, "ann@acme.test").await;
    let app = app(st);
    let cookie = login(&app, "acme", "ann@acme.test").await.unwrap();
    let code = || get_code(&app, "acme", &cookie, "app", REDIRECT, "openid offline_access");

    let (status, body) = exchange(&app, "acme", &code_grant(&code().await, "the-wrong-verifier-the-wrong-verifier-the-wrong")).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_grant")));

    let wrong_redirect = format!("grant_type=authorization_code&client_id=app&code={}&code_verifier={VERIFIER}&redirect_uri=https%3A%2F%2Fevil.test%2Fcb", code().await);
    assert_eq!(exchange(&app, "acme", &wrong_redirect).await.0, StatusCode::BAD_REQUEST);

    // a redirect_uri that is not registered is never redirected to
    let req = Request::get(authorize_url("acme", "app", "https://evil.test/cb", "openid")).header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
    let (status, headers, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!headers.contains_key(header::LOCATION));

    // replaying a code fails and revokes the refresh token the first use produced
    let code = code().await;
    let (status, first) = exchange(&app, "acme", &code_grant(&code, VERIFIER)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exchange(&app, "acme", &code_grant(&code, VERIFIER)).await.0, StatusCode::BAD_REQUEST);
    let refresh = format!("grant_type=refresh_token&client_id=app&refresh_token={}", first["refresh_token"].as_str().unwrap());
    assert_eq!(exchange(&app, "acme", &refresh).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn refresh_tokens_rotate_and_reuse_revokes_the_family(db: PgPool) {
    let st = state(db);
    let t = mk_tenant(&st, "acme").await;
    mk_user(&st, &t, "ann@acme.test").await;
    let app = app(st);
    let first = tokens(&app, "acme", "ann@acme.test", "openid offline_access").await;
    let grant = |token: &Value| format!("grant_type=refresh_token&client_id=app&refresh_token={}", token.as_str().unwrap());

    let (status, second) = exchange(&app, "acme", &grant(&first["refresh_token"])).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_ne!(first["refresh_token"], second["refresh_token"]);

    // the rotated-out token is replayed: it fails, and so does its successor
    assert_eq!(exchange(&app, "acme", &grant(&first["refresh_token"])).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(exchange(&app, "acme", &grant(&second["refresh_token"])).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn tenants_are_isolated(db: PgPool) {
    let st = state(db);
    let a = mk_tenant(&st, "a").await;
    let b = mk_tenant(&st, "b").await;
    let ann = mk_user(&st, &a, "ann@a.test").await;
    mk_user(&st, &b, "bob@b.test").await;
    bootstrap(&st, Some("root@admin.test".into()), Some(PASSWORD.into())).await.unwrap();
    // even with manage:all granted by a role in her own tenant, ann is no console operator
    sqlx::query("with r as (insert into role (tenant_id, name, scopes) values ($1, 'fake-admin', '{manage:all}') returning id) insert into user_role select $2, id from r")
        .bind(a.id)
        .bind(ann)
        .execute(&st.db)
        .await
        .unwrap();
    let app = app(st);

    // credentials from tenant A do not sign in to tenant B
    assert!(login(&app, "b", "ann@a.test").await.is_none());

    // a tenant A session does not authorize in tenant B
    let cookie = login(&app, "a", "ann@a.test").await.unwrap();
    let req = Request::get(authorize_url("b", "app", REDIRECT, "openid")).header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
    let (_, headers, _) = send(&app, req).await;
    assert!(location(&headers).starts_with("/auth/t/b/sign-in"));

    // a tenant A token is rejected by tenant B and by the management API
    let code = get_code(&app, "a", &cookie, "app", REDIRECT, "openid manage:all").await;
    let (_, body) = exchange(&app, "a", &code_grant(&code, VERIFIER)).await;
    assert_eq!(body["scope"], "openid manage:all");
    let token = body["access_token"].as_str().unwrap();
    assert_eq!(userinfo(&app, "a", token).await.0, StatusCode::OK);
    assert_eq!(userinfo(&app, "b", token).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app, api("GET", "/auth/api/tenants", token, json!({}))).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app, Request::get("/auth/api/tenants").body(Body::empty()).unwrap()).await.0, StatusCode::UNAUTHORIZED);
}

/// Signs the bootstrap operator in through the console's own client.
async fn admin_token(app: &Router) -> String {
    let cookie = login(app, "admin", "root@admin.test").await.expect("admin login");
    let code = get_code(app, "admin", &cookie, "web-accounts", "https://console.test/callback", "openid manage:all").await;
    let grant = format!(
        "grant_type=authorization_code&client_id=web-accounts&code={code}&code_verifier={VERIFIER}&redirect_uri={}",
        urlencode("https://console.test/callback")
    );
    let (status, body) = exchange(app, "admin", &grant).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scope"], "openid manage:all");
    body["access_token"].as_str().unwrap().to_string()
}

#[sqlx::test]
async fn management_api_manages_a_tenant(db: PgPool) {
    let st = state(db);
    bootstrap(&st, Some("root@admin.test".into()), Some(PASSWORD.into())).await.unwrap();
    let app = app(st.clone());
    let token = admin_token(&app).await;
    let call = |method: &'static str, path: &str, body: Value| {
        let req = api(method, &format!("/auth/api{path}"), &token, body);
        let app = app.clone();
        async move {
            let (status, _, body) = send(&app, req).await;
            (status, serde_json::from_str::<Value>(&body).unwrap_or(Value::Null))
        }
    };

    let (status, _) = call("POST", "/tenants", json!({ "slug": "acme", "name": "Acme" })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(call("POST", "/tenants", json!({ "slug": "acme", "name": "Again" })).await.0, StatusCode::CONFLICT);
    assert_eq!(call("POST", "/tenants", json!({ "slug": "Not A Slug", "name": "x" })).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call("DELETE", "/tenants/admin", json!({})).await.0, StatusCode::BAD_REQUEST);

    let (status, user) = call("POST", "/tenants/acme/users", json!({ "email": "Ann@Acme.test", "password": PASSWORD })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(user["email"], "ann@acme.test");
    assert!(user.get("password_hash").is_none());
    let uid = user["id"].as_str().unwrap();

    let (_, role) = call("POST", "/tenants/acme/roles", json!({ "name": "editor", "scopes": ["posts:write"] })).await;
    assert_eq!(call("POST", "/tenants/acme/roles", json!({ "name": "bad", "scopes": ["openid"] })).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(call("PUT", &format!("/tenants/acme/users/{uid}/roles"), json!({ "role_ids": [role["id"]] })).await.0, StatusCode::NO_CONTENT);

    let (_, org) = call("POST", "/tenants/acme/organizations", json!({ "name": "Team" })).await;
    let member = format!("/tenants/acme/organizations/{}/members/{uid}", org["id"].as_str().unwrap());
    assert_eq!(call("PUT", &member, json!({ "role": "owner" })).await.0, StatusCode::NO_CONTENT);

    let (status, created) = call("POST", "/tenants/acme/applications", json!({ "name": "Site", "confidential": true, "redirect_uris": [REDIRECT] })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(created["client_secret"].is_string());
    assert_eq!(call("POST", "/tenants/acme/applications", json!({ "name": "Bad", "redirect_uris": ["not a url"] })).await.0, StatusCode::BAD_REQUEST);
    let (_, apps) = call("GET", "/tenants/acme/applications", json!({})).await;
    assert!(apps[0].get("client_secret").is_none() && apps[0].get("secret_hash").is_none());

    assert_eq!(call("PUT", "/tenants/acme/connectors/github", json!({ "client_id": "gh", "client_secret": "s3cret" })).await.0, StatusCode::OK);
    assert_eq!(call("PUT", "/tenants/acme/connectors/myspace", json!({ "client_id": "x", "client_secret": "y" })).await.0, StatusCode::BAD_REQUEST);
    let (_, connectors) = call("GET", "/tenants/acme/connectors", json!({})).await;
    assert!(!connectors.to_string().contains("s3cret"));

    // the new user signs in through the confidential client and gets role scopes and organizations
    let (client_id, secret) = (created["client_id"].as_str().unwrap(), created["client_secret"].as_str().unwrap());
    let cookie = login(&app, "acme", "ann@acme.test").await.unwrap();
    let code = get_code(&app, "acme", &cookie, client_id, REDIRECT, "openid organizations offline_access posts:write").await;
    let grant = format!("grant_type=authorization_code&code={code}&code_verifier={VERIFIER}&redirect_uri={}&client_id={client_id}", urlencode(REDIRECT));
    // client authentication fails before the code is touched, so the code is still good afterwards
    assert_eq!(exchange(&app, "acme", &grant).await.0, StatusCode::UNAUTHORIZED, "secret is required");
    let (status, body) = exchange(&app, "acme", &format!("{grant}&client_secret={secret}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scope"], "openid organizations offline_access posts:write");
    let (_, info) = userinfo(&app, "acme", body["access_token"].as_str().unwrap()).await;
    assert!(info.contains("\"role\":\"owner\""), "{info}");

    // suspending signs the user out everywhere
    assert_eq!(call("PATCH", &format!("/tenants/acme/users/{uid}"), json!({ "suspended": true })).await.0, StatusCode::OK);
    assert!(login(&app, "acme", "ann@acme.test").await.is_none());
    let refresh = format!("grant_type=refresh_token&client_id={client_id}&client_secret={secret}&refresh_token={}", body["refresh_token"].as_str().unwrap());
    assert_eq!(exchange(&app, "acme", &refresh).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(userinfo(&app, "acme", body["access_token"].as_str().unwrap()).await.0, StatusCode::UNAUTHORIZED);

    // a later start with ADMIN_PASSWORD set restores the operator with that password; without it nothing changes
    bootstrap(&st, Some("root@admin.test".into()), None).await.unwrap();
    assert!(login(&app, "admin", "root@admin.test").await.is_some());
    bootstrap(&st, Some("root@admin.test".into()), Some("a-replacement-password".into())).await.unwrap();
    assert!(login(&app, "admin", "root@admin.test").await.is_none(), "old password replaced");
    assert_eq!(call("GET", "/tenants", json!({})).await.0, StatusCode::OK, "still an admin");

    // rotating the admin tenant's key invalidates the operator's token at once
    assert_eq!(call("POST", "/tenants/admin/keys", json!({})).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call("GET", "/tenants", json!({})).await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn hosted_forms_resist_csrf_enumeration_and_open_redirects(db: PgPool) {
    let st = state(db);
    let t = mk_tenant(&st, "acme").await;
    mk_user(&st, &t, "ann@acme.test").await;
    let app = app(st.clone());

    // a form post from another origin (or with none) is refused
    let foreign = Request::post("/auth/t/acme/sign-in")
        .header(header::ORIGIN, "https://evil.test")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("email=ann@acme.test&password={PASSWORD}")))
        .unwrap();
    assert_eq!(send(&app, foreign).await.0, StatusCode::FORBIDDEN);

    // unknown email and wrong password are indistinguishable
    let wrong = send(&app, form("/auth/t/acme/sign-in", "email=ann@acme.test&password=nope-nope-nope")).await;
    let unknown = send(&app, form("/auth/t/acme/sign-in", "email=nobody@acme.test&password=nope-nope-nope")).await;
    assert_eq!((wrong.0, &wrong.2), (StatusCode::UNAUTHORIZED, &unknown.2));

    // return_to outside this tenant's authorize endpoint is ignored
    let (status, headers, _) = send(&app, form("/auth/t/acme/sign-in?return_to=https%3A%2F%2Fevil.test", &format!("email=ann@acme.test&password={PASSWORD}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!headers.contains_key(header::LOCATION));

    // signing up with a taken address answers exactly like a new one and changes nothing
    let taken = send(&app, form("/auth/t/acme/sign-up", "name=x&email=ann@acme.test&password=another-password")).await;
    let fresh = send(&app, form("/auth/t/acme/sign-up", "name=x&email=new@acme.test&password=another-password")).await;
    assert_eq!((taken.0, &taken.2), (fresh.0, &fresh.2));
    assert!(login(&app, "acme", "ann@acme.test").await.is_some());

    // password reset: single use, and it ends existing sessions
    let cookie = login(&app, "acme", "ann@acme.test").await.unwrap();
    let user_id: Uuid = sqlx::query_scalar("select id from app_user where email = 'ann@acme.test'").fetch_one(&st.db).await.unwrap();
    let token = identity::create_verification(&st.db, t.id, user_id, "reset_password", 1).await.unwrap();
    let reset = format!("/auth/t/acme/reset-password?token={token}");
    assert_eq!(send(&app, form(&reset, "password=short")).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(send(&app, form(&reset, "password=a-brand-new-password")).await.0, StatusCode::OK);
    assert_eq!(send(&app, form(&reset, "password=yet-another-password")).await.0, StatusCode::BAD_REQUEST);
    assert!(login(&app, "acme", "ann@acme.test").await.is_none(), "old password no longer works");
    let req = Request::get(authorize_url("acme", "app", REDIRECT, "openid")).header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
    assert!(location(&send(&app, req).await.1).starts_with("/auth/t/acme/sign-in"), "old session was revoked");
}

#[sqlx::test]
async fn social_linking_trusts_only_verified_emails(db: PgPool) {
    let st = state(db);
    let t = mk_tenant(&st, "acme").await;
    let profile = |id: &str, email: Option<&str>| Profile { id: id.into(), email: email.map(str::to_string), name: "Ann".into() };

    // no verified email: nothing is created or linked
    assert!(link_or_create(&st.db, &t, "github", profile("1", None)).await.unwrap().is_err());

    // a squatter pre-registered the address without verifying it; the real owner arrives via the provider
    let hash = identity::hash_password(PASSWORD.into()).await.unwrap();
    let squatted: Uuid = sqlx::query_scalar("insert into app_user (tenant_id, email, password_hash) values ($1, 'ann@acme.test', $2) returning id")
        .bind(t.id)
        .bind(hash)
        .fetch_one(&st.db)
        .await
        .unwrap();
    let user = link_or_create(&st.db, &t, "github", profile("1", Some("Ann@Acme.test"))).await.unwrap().unwrap();
    assert_eq!(user.id, squatted);
    assert!(user.email_verified && user.password_hash.is_none(), "squatter's password is gone");

    // the same provider account maps to the same user from then on, whatever email it reports
    let again = link_or_create(&st.db, &t, "github", profile("1", None)).await.unwrap().unwrap();
    assert_eq!(again.id, squatted);

    // a new verified address creates a user; the same provider id under another provider is a different account
    let other = link_or_create(&st.db, &t, "google", profile("1", Some("new@acme.test"))).await.unwrap().unwrap();
    assert_ne!(other.id, squatted);
}
