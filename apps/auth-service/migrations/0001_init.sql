-- Every table below `tenant` carries tenant_id, and every query filters on it.

create table tenant (
    id         uuid primary key default gen_random_uuid(),
    slug       text not null unique check (slug ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    name       text not null check (name <> ''),
    config     jsonb not null default '{}',
    created_at timestamptz not null default now()
);

create table app_user (
    id             uuid primary key default gen_random_uuid(),
    tenant_id      uuid not null references tenant on delete cascade,
    -- normalized in Rust (identity::normalize_email); the check keeps it honest
    email          text not null check (email = lower(email) and email <> ''),
    email_verified boolean not null default false,
    password_hash  text,
    name           text not null default '',
    suspended      boolean not null default false,
    created_at     timestamptz not null default now(),
    unique (tenant_id, email)
);

-- Social identities linked to a user.
create table account (
    id                  uuid primary key default gen_random_uuid(),
    tenant_id           uuid not null references tenant on delete cascade,
    user_id             uuid not null references app_user on delete cascade,
    provider            text not null,
    provider_account_id text not null,
    created_at          timestamptz not null default now(),
    unique (tenant_id, provider, provider_account_id)
);

-- Browser sessions for the hosted pages. Only the SHA-256 of the cookie value is stored.
create table session (
    id         uuid primary key default gen_random_uuid(),
    token_hash bytea not null unique,
    tenant_id  uuid not null references tenant on delete cascade,
    user_id    uuid not null references app_user on delete cascade,
    ip         text,
    user_agent text,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);
create index on session (user_id);

create table verification (
    token_hash bytea primary key,
    tenant_id  uuid not null references tenant on delete cascade,
    user_id    uuid not null references app_user on delete cascade,
    purpose    text not null check (purpose in ('verify_email', 'reset_password')),
    expires_at timestamptz not null
);

create table role (
    id          uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenant on delete cascade,
    name        text not null check (name <> ''),
    description text not null default '',
    scopes      text[] not null default '{}',
    unique (tenant_id, name)
);

create table user_role (
    user_id uuid not null references app_user on delete cascade,
    role_id uuid not null references role on delete cascade,
    primary key (user_id, role_id)
);

create table organization (
    id         uuid primary key default gen_random_uuid(),
    tenant_id  uuid not null references tenant on delete cascade,
    name       text not null check (name <> ''),
    created_at timestamptz not null default now(),
    unique (tenant_id, name)
);

create table org_member (
    org_id  uuid not null references organization on delete cascade,
    user_id uuid not null references app_user on delete cascade,
    role    text not null default 'member' check (role <> ''),
    primary key (org_id, user_id)
);

create table oidc_client (
    id               uuid primary key default gen_random_uuid(),
    tenant_id        uuid not null references tenant on delete cascade,
    client_id        text not null check (client_id <> ''),
    name             text not null check (name <> ''),
    -- null = public client (PKCE only)
    secret_hash      bytea,
    redirect_uris    text[] not null default '{}',
    post_logout_uris text[] not null default '{}',
    created_at       timestamptz not null default now(),
    unique (tenant_id, client_id)
);

create table auth_code (
    code_hash      bytea primary key,
    tenant_id      uuid not null references tenant on delete cascade,
    client_id      uuid not null references oidc_client on delete cascade,
    user_id        uuid not null references app_user on delete cascade,
    redirect_uri   text not null,
    scope          text not null,
    code_challenge text not null,
    nonce          text,
    -- refresh tokens minted from this code share the family, so a replayed code can revoke them
    family_id      uuid not null default gen_random_uuid(),
    auth_time      timestamptz not null,
    expires_at     timestamptz not null,
    used_at        timestamptz
);

create table refresh_token (
    token_hash bytea primary key,
    tenant_id  uuid not null references tenant on delete cascade,
    family_id  uuid not null,
    client_id  uuid not null references oidc_client on delete cascade,
    user_id    uuid not null references app_user on delete cascade,
    scope      text not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null,
    revoked_at timestamptz
);
create index on refresh_token (family_id);
create index on refresh_token (user_id);

create table signing_key (
    kid         uuid primary key default gen_random_uuid(),
    tenant_id   uuid not null references tenant on delete cascade,
    -- ponytail: stored unencrypted, as are social_connector secrets. Upgrade: envelope-encrypt with a key from env.
    private_pem text not null,
    -- base64url RSA modulus and exponent, ready for JWKS
    n           text not null,
    e           text not null,
    created_at  timestamptz not null default now(),
    retired_at  timestamptz
);
create unique index on signing_key (tenant_id) where retired_at is null;

create table social_connector (
    tenant_id     uuid not null references tenant on delete cascade,
    provider      text not null check (provider in ('google', 'github')),
    client_id     text not null check (client_id <> ''),
    client_secret text not null check (client_secret <> ''),
    enabled       boolean not null default true,
    primary key (tenant_id, provider)
);
