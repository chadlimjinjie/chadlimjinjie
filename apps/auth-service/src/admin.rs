//! Management API for the console. Every route requires an admin-tenant access token with `manage:all`.

use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    middleware::from_extractor_with_state,
    routing::{get, patch, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{types::Json as Jsonb, PgPool};
use url::Url;
use uuid::Uuid;

use crate::{
    error::{bad, AppError, Result},
    identity::{self, random_token, sha256, User},
    keys,
    oidc::{bearer, Client, CLIENT_COLS, OIDC_SCOPES},
    tenant::{self, Tenant, TenantConfig},
    AppState,
};

pub const ADMIN_TENANT: &str = "admin";
pub const MANAGE_SCOPE: &str = "manage:all";

/// The authenticated console operator.
pub struct Admin {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self> {
        let token = bearer(&parts.headers).ok_or(AppError::Unauthorized)?;
        // Verified against the admin tenant's key and issuer only: tokens from any other tenant fail here.
        let admin = Tenant::by_slug(&st.db, ADMIN_TENANT).await?.ok_or(AppError::Unauthorized)?;
        let claims = keys::verify_access(st, &admin, token).await.ok_or(AppError::Unauthorized)?;
        if !claims.has_scope(MANAGE_SCOPE) {
            return Err(AppError::Forbidden);
        }
        // Checked per request, so suspending an operator takes effect before their token expires.
        let active: Option<Uuid> = sqlx::query_scalar("select id from app_user where id = $1 and tenant_id = $2 and not suspended")
            .bind(claims.sub)
            .bind(admin.id)
            .fetch_optional(&st.db)
            .await?;
        Ok(Admin { user_id: active.ok_or(AppError::Unauthorized)? })
    }
}

pub fn router(state: AppState) -> Router<AppState> {
    let tenant = Router::new()
        .route("/", get(get_tenant).patch(update_tenant).delete(delete_tenant))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user).patch(update_user).delete(delete_user))
        .route("/users/{id}/roles", put(set_user_roles))
        .route("/users/{id}/sessions", axum::routing::delete(revoke_sessions))
        .route("/roles", get(list_roles).post(create_role))
        .route("/roles/{id}", patch(update_role).delete(delete_role))
        .route("/organizations", get(list_orgs).post(create_org))
        .route("/organizations/{id}", patch(update_org).delete(delete_org))
        .route("/organizations/{id}/members", get(list_members))
        .route("/organizations/{id}/members/{user_id}", put(put_member).delete(delete_member))
        .route("/applications", get(list_apps).post(create_app))
        .route("/applications/{id}", patch(update_app).delete(delete_app))
        .route("/applications/{id}/secret", post(rotate_secret))
        .route("/connectors", get(list_connectors))
        .route("/connectors/{provider}", put(put_connector).delete(delete_connector))
        .route("/keys", get(list_keys).post(rotate_key));
    Router::new()
        .route("/tenants", get(list_tenants).post(create_tenant))
        .nest("/tenants/{tenant}", tenant)
        .route_layer(from_extractor_with_state::<Admin, _>(state))
}

#[derive(Deserialize)]
struct Id {
    id: Uuid,
}

fn name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(bad("name must be between 1 and 200 characters"));
    }
    Ok(value.to_string())
}

/// Deletes one tenant-owned row; the table name is always a literal from this file.
async fn remove(db: &PgPool, table: &'static str, tenant_id: Uuid, id: Uuid) -> Result<StatusCode> {
    let done = sqlx::query(&format!("delete from {table} where id = $1 and tenant_id = $2"))
        .bind(id)
        .bind(tenant_id)
        .execute(db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---- tenants ----

async fn list_tenants(State(st): State<AppState>) -> Result<Json<Vec<Tenant>>> {
    Ok(Json(sqlx::query_as("select * from tenant order by created_at").fetch_all(&st.db).await?))
}

#[derive(Deserialize)]
struct NewTenant {
    slug: String,
    name: String,
}

async fn create_tenant(State(st): State<AppState>, Json(body): Json<NewTenant>) -> Result<(StatusCode, Json<Tenant>)> {
    let mut tx = st.db.begin().await?;
    let created = tenant::create(&mut tx, &body.slug, &name(&body.name)?, TenantConfig::default()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn get_tenant(tenant: Tenant) -> Json<Tenant> {
    Json(tenant)
}

#[derive(Deserialize)]
struct TenantPatch {
    name: Option<String>,
    config: Option<TenantConfig>,
}

async fn update_tenant(State(st): State<AppState>, tenant: Tenant, Json(body): Json<TenantPatch>) -> Result<Json<Tenant>> {
    let new_name = body.name.as_deref().map(name).transpose()?;
    if let Some(config) = &body.config {
        config.validate()?;
    }
    Ok(Json(
        sqlx::query_as("update tenant set name = coalesce($2, name), config = coalesce($3, config) where id = $1 returning *")
            .bind(tenant.id)
            .bind(new_name)
            .bind(body.config.map(Jsonb))
            .fetch_one(&st.db)
            .await?,
    ))
}

async fn delete_tenant(State(st): State<AppState>, tenant: Tenant) -> Result<StatusCode> {
    if tenant.slug == ADMIN_TENANT {
        return Err(bad("the admin tenant cannot be deleted"));
    }
    sqlx::query("delete from tenant where id = $1").bind(tenant.id).execute(&st.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- users ----

#[derive(Deserialize)]
struct Search {
    q: Option<String>,
}

async fn list_users(State(st): State<AppState>, tenant: Tenant, Query(s): Query<Search>) -> Result<Json<Vec<User>>> {
    // ponytail: newest 200 matches, no paging. Upgrade: keyset pagination on (created_at, id) once a tenant outgrows it.
    Ok(Json(
        sqlx::query_as(
            "select * from app_user where tenant_id = $1
             and ($2 = '' or position($2 in email) > 0 or position($2 in lower(name)) > 0)
             order by created_at desc limit 200",
        )
        .bind(tenant.id)
        .bind(s.q.unwrap_or_default().trim().to_lowercase())
        .fetch_all(&st.db)
        .await?,
    ))
}

async fn find_user(db: &PgPool, tenant: &Tenant, id: Uuid) -> Result<User> {
    Ok(sqlx::query_as("select * from app_user where id = $1 and tenant_id = $2").bind(id).bind(tenant.id).fetch_one(db).await?)
}

#[derive(Deserialize)]
struct NewUser {
    email: String,
    #[serde(default)]
    name: String,
    password: Option<String>,
    #[serde(default)]
    email_verified: bool,
}

async fn create_user(State(st): State<AppState>, tenant: Tenant, Json(body): Json<NewUser>) -> Result<(StatusCode, Json<User>)> {
    let email = identity::normalize_email(&body.email).ok_or_else(|| bad("invalid email address"))?;
    let hash = match body.password {
        Some(password) => {
            identity::check_password(&tenant, &password)?;
            Some(identity::hash_password(password).await?)
        }
        None => None,
    };
    let user = sqlx::query_as(
        "insert into app_user (tenant_id, email, name, password_hash, email_verified) values ($1, $2, $3, $4, $5) returning *",
    )
    .bind(tenant.id)
    .bind(email)
    .bind(body.name.trim().chars().take(200).collect::<String>())
    .bind(hash)
    .bind(body.email_verified)
    .fetch_one(&st.db)
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Serialize, sqlx::FromRow)]
struct SessionInfo {
    id: Uuid,
    ip: Option<String>,
    user_agent: Option<String>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct UserDetail {
    #[serde(flatten)]
    user: User,
    has_password: bool,
    role_ids: Vec<Uuid>,
    providers: Vec<String>,
    sessions: Vec<SessionInfo>,
}

async fn get_user(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<Json<UserDetail>> {
    let user = find_user(&st.db, &tenant, p.id).await?;
    let role_ids = sqlx::query_scalar("select role_id from user_role where user_id = $1").bind(user.id).fetch_all(&st.db).await?;
    let providers =
        sqlx::query_scalar("select provider from account where user_id = $1 order by provider").bind(user.id).fetch_all(&st.db).await?;
    let sessions = sqlx::query_as(
        "select id, ip, user_agent, created_at, expires_at from session where user_id = $1 and expires_at > now() order by created_at desc",
    )
    .bind(user.id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(UserDetail { has_password: user.password_hash.is_some(), user, role_ids, providers, sessions }))
}

#[derive(Deserialize)]
struct UserPatch {
    email: Option<String>,
    name: Option<String>,
    email_verified: Option<bool>,
    suspended: Option<bool>,
    password: Option<String>,
}

async fn update_user(
    State(st): State<AppState>,
    admin: Admin,
    tenant: Tenant,
    Path(p): Path<Id>,
    Json(body): Json<UserPatch>,
) -> Result<Json<User>> {
    if body.suspended == Some(true) && p.id == admin.user_id {
        return Err(bad("you cannot suspend your own account"));
    }
    let email = match &body.email {
        Some(email) => Some(identity::normalize_email(email).ok_or_else(|| bad("invalid email address"))?),
        None => None,
    };
    let hash = match body.password {
        Some(password) => {
            identity::check_password(&tenant, &password)?;
            Some(identity::hash_password(password).await?)
        }
        None => None,
    };
    let sign_out = body.suspended == Some(true) || hash.is_some();
    let user: User = sqlx::query_as(
        "update app_user set email = coalesce($3, email), name = coalesce($4, name),
            email_verified = coalesce($5, email_verified), suspended = coalesce($6, suspended),
            password_hash = coalesce($7, password_hash)
         where id = $1 and tenant_id = $2 returning *",
    )
    .bind(p.id)
    .bind(tenant.id)
    .bind(email)
    .bind(body.name.map(|n| n.trim().chars().take(200).collect::<String>()))
    .bind(body.email_verified)
    .bind(body.suspended)
    .bind(hash)
    .fetch_one(&st.db)
    .await?;
    if sign_out {
        identity::revoke_user(&st.db, user.id).await?;
    }
    Ok(Json(user))
}

async fn delete_user(State(st): State<AppState>, admin: Admin, tenant: Tenant, Path(p): Path<Id>) -> Result<StatusCode> {
    if p.id == admin.user_id {
        return Err(bad("you cannot delete your own account"));
    }
    remove(&st.db, "app_user", tenant.id, p.id).await
}

#[derive(Deserialize)]
struct RoleIds {
    role_ids: Vec<Uuid>,
}

async fn set_user_roles(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>, Json(body): Json<RoleIds>) -> Result<StatusCode> {
    let user = find_user(&st.db, &tenant, p.id).await?;
    let mut tx = st.db.begin().await?;
    sqlx::query("delete from user_role where user_id = $1").bind(user.id).execute(&mut *tx).await?;
    // selecting through `role` drops ids that belong to another tenant
    sqlx::query("insert into user_role (user_id, role_id) select $1, id from role where tenant_id = $2 and id = any($3)")
        .bind(user.id)
        .bind(tenant.id)
        .bind(&body.role_ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_sessions(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<StatusCode> {
    let user = find_user(&st.db, &tenant, p.id).await?;
    identity::revoke_user(&st.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- roles ----

#[derive(Serialize, sqlx::FromRow)]
struct Role {
    id: Uuid,
    name: String,
    description: String,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct RoleBody {
    name: Option<String>,
    description: Option<String>,
    scopes: Option<Vec<String>>,
}

fn check_scopes(scopes: &[String]) -> Result<()> {
    let ok = scopes
        .iter()
        .all(|s| !s.is_empty() && s.len() <= 100 && !s.chars().any(|c| c.is_whitespace() || c.is_control()) && !OIDC_SCOPES.contains(&s.as_str()));
    ok.then_some(()).ok_or_else(|| bad("scopes must be non-empty, without spaces, and not a built-in OIDC scope"))
}

async fn list_roles(State(st): State<AppState>, tenant: Tenant) -> Result<Json<Vec<Role>>> {
    Ok(Json(
        sqlx::query_as("select id, name, description, scopes from role where tenant_id = $1 order by name")
            .bind(tenant.id)
            .fetch_all(&st.db)
            .await?,
    ))
}

async fn create_role(State(st): State<AppState>, tenant: Tenant, Json(body): Json<RoleBody>) -> Result<(StatusCode, Json<Role>)> {
    let scopes = body.scopes.unwrap_or_default();
    check_scopes(&scopes)?;
    let role = sqlx::query_as(
        "insert into role (tenant_id, name, description, scopes) values ($1, $2, $3, $4) returning id, name, description, scopes",
    )
    .bind(tenant.id)
    .bind(name(body.name.as_deref().unwrap_or_default())?)
    .bind(body.description.unwrap_or_default())
    .bind(scopes)
    .fetch_one(&st.db)
    .await?;
    Ok((StatusCode::CREATED, Json(role)))
}

async fn update_role(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>, Json(body): Json<RoleBody>) -> Result<Json<Role>> {
    if let Some(scopes) = &body.scopes {
        check_scopes(scopes)?;
    }
    Ok(Json(
        sqlx::query_as(
            "update role set name = coalesce($3, name), description = coalesce($4, description), scopes = coalesce($5, scopes)
             where id = $1 and tenant_id = $2 returning id, name, description, scopes",
        )
        .bind(p.id)
        .bind(tenant.id)
        .bind(body.name.as_deref().map(name).transpose()?)
        .bind(body.description)
        .bind(body.scopes)
        .fetch_one(&st.db)
        .await?,
    ))
}

async fn delete_role(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<StatusCode> {
    remove(&st.db, "role", tenant.id, p.id).await
}

// ---- organizations ----

#[derive(Serialize, sqlx::FromRow)]
struct Org {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    members: i64,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

const ORG_COLS: &str = "id, name, created_at, (select count(*) from org_member m where m.org_id = organization.id) as members";

async fn list_orgs(State(st): State<AppState>, tenant: Tenant) -> Result<Json<Vec<Org>>> {
    Ok(Json(
        sqlx::query_as(&format!("select {ORG_COLS} from organization where tenant_id = $1 order by name"))
            .bind(tenant.id)
            .fetch_all(&st.db)
            .await?,
    ))
}

async fn create_org(State(st): State<AppState>, tenant: Tenant, Json(body): Json<Named>) -> Result<(StatusCode, Json<Org>)> {
    let org = sqlx::query_as(&format!("insert into organization (tenant_id, name) values ($1, $2) returning {ORG_COLS}"))
        .bind(tenant.id)
        .bind(name(&body.name)?)
        .fetch_one(&st.db)
        .await?;
    Ok((StatusCode::CREATED, Json(org)))
}

async fn update_org(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>, Json(body): Json<Named>) -> Result<Json<Org>> {
    Ok(Json(
        sqlx::query_as(&format!("update organization set name = $3 where id = $1 and tenant_id = $2 returning {ORG_COLS}"))
            .bind(p.id)
            .bind(tenant.id)
            .bind(name(&body.name)?)
            .fetch_one(&st.db)
            .await?,
    ))
}

async fn delete_org(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<StatusCode> {
    remove(&st.db, "organization", tenant.id, p.id).await
}

#[derive(Serialize, sqlx::FromRow)]
struct Member {
    user_id: Uuid,
    email: String,
    name: String,
    role: String,
}

async fn list_members(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<Json<Vec<Member>>> {
    Ok(Json(
        sqlx::query_as(
            "select u.id as user_id, u.email, u.name, m.role from org_member m
             join organization o on o.id = m.org_id join app_user u on u.id = m.user_id
             where o.id = $1 and o.tenant_id = $2 order by u.email",
        )
        .bind(p.id)
        .bind(tenant.id)
        .fetch_all(&st.db)
        .await?,
    ))
}

#[derive(Deserialize)]
struct MemberPath {
    id: Uuid,
    user_id: Uuid,
}

#[derive(Deserialize)]
struct MemberBody {
    role: Option<String>,
}

async fn put_member(
    State(st): State<AppState>,
    tenant: Tenant,
    Path(p): Path<MemberPath>,
    Json(body): Json<MemberBody>,
) -> Result<StatusCode> {
    let role = body.role.as_deref().map(name).transpose()?.unwrap_or_else(|| "member".into());
    // the joins require the organization and the user to both belong to this tenant
    let done = sqlx::query(
        "insert into org_member (org_id, user_id, role)
         select o.id, u.id, $4 from organization o, app_user u
         where o.id = $1 and u.id = $2 and o.tenant_id = $3 and u.tenant_id = $3
         on conflict (org_id, user_id) do update set role = excluded.role",
    )
    .bind(p.id)
    .bind(p.user_id)
    .bind(tenant.id)
    .bind(role)
    .execute(&st.db)
    .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_member(State(st): State<AppState>, tenant: Tenant, Path(p): Path<MemberPath>) -> Result<StatusCode> {
    sqlx::query(
        "delete from org_member m using organization o
         where o.id = m.org_id and m.org_id = $1 and m.user_id = $2 and o.tenant_id = $3",
    )
    .bind(p.id)
    .bind(p.user_id)
    .bind(tenant.id)
    .execute(&st.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- applications (OIDC clients) ----

/// Redirect targets are matched exactly, so they must be absolute and carry no fragment.
fn check_uris(uris: &[String]) -> Result<()> {
    let ok = uris.iter().all(|u| Url::parse(u).is_ok_and(|url| url.fragment().is_none()));
    ok.then_some(()).ok_or_else(|| bad("URIs must be absolute and have no fragment"))
}

async fn list_apps(State(st): State<AppState>, tenant: Tenant) -> Result<Json<Vec<Client>>> {
    Ok(Json(
        sqlx::query_as(&format!("select {CLIENT_COLS} from oidc_client where tenant_id = $1 order by name"))
            .bind(tenant.id)
            .fetch_all(&st.db)
            .await?,
    ))
}

#[derive(Deserialize)]
struct NewApp {
    name: String,
    #[serde(default)]
    confidential: bool,
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    post_logout_uris: Vec<String>,
}

/// The secret is returned once, here; only its hash is stored.
async fn create_app(State(st): State<AppState>, tenant: Tenant, Json(body): Json<NewApp>) -> Result<(StatusCode, Json<Value>)> {
    check_uris(&body.redirect_uris)?;
    check_uris(&body.post_logout_uris)?;
    let secret = body.confidential.then(random_token);
    let client: Client = sqlx::query_as(&format!(
        "insert into oidc_client (tenant_id, client_id, name, secret_hash, redirect_uris, post_logout_uris)
         values ($1, $2, $3, $4, $5, $6) returning {CLIENT_COLS}"
    ))
    .bind(tenant.id)
    .bind(random_token()[..22].to_string())
    .bind(name(&body.name)?)
    .bind(secret.as_deref().map(sha256))
    .bind(body.redirect_uris)
    .bind(body.post_logout_uris)
    .fetch_one(&st.db)
    .await?;
    let mut out = json!(client);
    out["client_secret"] = json!(secret);
    Ok((StatusCode::CREATED, Json(out)))
}

#[derive(Deserialize)]
struct AppPatch {
    name: Option<String>,
    redirect_uris: Option<Vec<String>>,
    post_logout_uris: Option<Vec<String>>,
}

async fn update_app(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>, Json(body): Json<AppPatch>) -> Result<Json<Client>> {
    check_uris(body.redirect_uris.as_deref().unwrap_or_default())?;
    check_uris(body.post_logout_uris.as_deref().unwrap_or_default())?;
    Ok(Json(
        sqlx::query_as(&format!(
            "update oidc_client set name = coalesce($3, name), redirect_uris = coalesce($4, redirect_uris),
                post_logout_uris = coalesce($5, post_logout_uris)
             where id = $1 and tenant_id = $2 returning {CLIENT_COLS}"
        ))
        .bind(p.id)
        .bind(tenant.id)
        .bind(body.name.as_deref().map(name).transpose()?)
        .bind(body.redirect_uris)
        .bind(body.post_logout_uris)
        .fetch_one(&st.db)
        .await?,
    ))
}

async fn delete_app(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<StatusCode> {
    remove(&st.db, "oidc_client", tenant.id, p.id).await
}

async fn rotate_secret(State(st): State<AppState>, tenant: Tenant, Path(p): Path<Id>) -> Result<Json<Value>> {
    let secret = random_token();
    // public clients have no secret to rotate
    let done = sqlx::query("update oidc_client set secret_hash = $3 where id = $1 and tenant_id = $2 and secret_hash is not null")
        .bind(p.id)
        .bind(tenant.id)
        .bind(sha256(&secret))
        .execute(&st.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "client_secret": secret })))
}

// ---- social connectors ----

#[derive(Serialize, sqlx::FromRow)]
struct Connector {
    provider: String,
    client_id: String,
    enabled: bool,
}

async fn list_connectors(State(st): State<AppState>, tenant: Tenant) -> Result<Json<Vec<Connector>>> {
    // the secret is write-only and never leaves the database
    Ok(Json(
        sqlx::query_as("select provider, client_id, enabled from social_connector where tenant_id = $1 order by provider")
            .bind(tenant.id)
            .fetch_all(&st.db)
            .await?,
    ))
}

#[derive(Deserialize)]
struct ProviderPath {
    provider: String,
}

#[derive(Deserialize)]
struct ConnectorBody {
    client_id: String,
    /// Omitted on update to keep the stored secret.
    client_secret: Option<String>,
    #[serde(default = "enabled")]
    enabled: bool,
}

fn enabled() -> bool {
    true
}

async fn put_connector(
    State(st): State<AppState>,
    tenant: Tenant,
    Path(p): Path<ProviderPath>,
    Json(body): Json<ConnectorBody>,
) -> Result<Json<Connector>> {
    let secret = body.client_secret.filter(|s| !s.is_empty());
    let updated: Option<Connector> = sqlx::query_as(
        "update social_connector set client_id = $3, client_secret = coalesce($4, client_secret), enabled = $5
         where tenant_id = $1 and provider = $2 returning provider, client_id, enabled",
    )
    .bind(tenant.id)
    .bind(&p.provider)
    .bind(body.client_id.trim())
    .bind(&secret)
    .bind(body.enabled)
    .fetch_optional(&st.db)
    .await?;
    if let Some(connector) = updated {
        return Ok(Json(connector));
    }
    let secret = secret.ok_or_else(|| bad("client_secret is required for a new connector"))?;
    // an unsupported provider fails the table's check constraint and comes back as a 400
    Ok(Json(
        sqlx::query_as(
            "insert into social_connector (tenant_id, provider, client_id, client_secret, enabled)
             values ($1, $2, $3, $4, $5) returning provider, client_id, enabled",
        )
        .bind(tenant.id)
        .bind(&p.provider)
        .bind(body.client_id.trim())
        .bind(secret)
        .bind(body.enabled)
        .fetch_one(&st.db)
        .await?,
    ))
}

async fn delete_connector(State(st): State<AppState>, tenant: Tenant, Path(p): Path<ProviderPath>) -> Result<StatusCode> {
    sqlx::query("delete from social_connector where tenant_id = $1 and provider = $2")
        .bind(tenant.id)
        .bind(p.provider)
        .execute(&st.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- signing keys ----

#[derive(Serialize, sqlx::FromRow)]
struct KeyInfo {
    kid: Uuid,
    created_at: DateTime<Utc>,
    retired_at: Option<DateTime<Utc>>,
}

async fn list_keys(State(st): State<AppState>, tenant: Tenant) -> Result<Json<Vec<KeyInfo>>> {
    Ok(Json(
        sqlx::query_as("select kid, created_at, retired_at from signing_key where tenant_id = $1 order by created_at desc")
            .bind(tenant.id)
            .fetch_all(&st.db)
            .await?,
    ))
}

async fn rotate_key(State(st): State<AppState>, tenant: Tenant) -> Result<StatusCode> {
    let mut tx = st.db.begin().await?;
    keys::generate(&mut tx, tenant.id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
