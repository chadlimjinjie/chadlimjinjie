# auth-service

Multi-tenant identity provider (axum + Postgres). Each tenant lives under `/auth/t/{slug}`:

- **Hosted pages + OIDC**: `/sign-in`, `/sign-up`, `/authorize`, `/token`, `/jwks`, … for apps that redirect to us.
- **Session API** (`/session/*`): JSON endpoints for tenants that build their own sign-in UI.
- **Management API**: `/auth/api/*`, which needs an admin-tenant token with `manage:all`.

## Session API (tenant-owned sign-in UI)

The tenant app proxies `/api/auth/*` on **its own origin** to `{AUTH_SERVICE_URL}/auth/t/{slug}/session/*`. In Next.js that's a `rewrites()` entry. The `auth_session` cookie is then first-party: it is host-only, HttpOnly, SameSite=Lax and `Path=/`, and it is Secure when `app_url` is https. That works on any domain without CORS or third-party cookies.

| Method | Path | Body | Result |
|---|---|---|---|
| POST | `sign-up` | `{name?, email, password}` | `{ok}`. The answer is the same for taken addresses, so sign in next. |
| POST | `sign-in` | `{email, password}` | Sets the cookie and returns the user `{id, email, name, email_verified, roles}`. A failed sign-in returns 401. |
| POST | `sign-out` | | Clears the cookie. |
| GET | `me` | | The user, or 401. |
| GET | `token` | | `{access_token, expires_in}`: an RS256 JWT (`sub`, `tenant`, `scope`, `roles`, `email`) for backends to verify against `/auth/t/{slug}/jwks`. |
| POST | `verify-email` | `{token}` | `{ok}`, or 400. |
| POST | `forgot-password` | `{email}` | Always `{ok}`. |
| POST | `reset-password` | `{token, password}` | `{ok}`, or 400. Signs the user out everywhere. |
| GET | `social/{google\|github}?return_to=` | | Full-page redirect to the provider. The callback comes back through the app's proxy. |

POSTs must carry an `Origin` that is either the tenant's `app_url` or one of its `allowed_origins`; anything else gets a 403. Mail links go to `{app_url}/verify-email?token=` and `{app_url}/reset-password?token=`, so the app serves those pages.

## Setting up a tenant

With an admin token (`Authorization: Bearer …`):

```sh
POST  /auth/api/tenants                     {"slug":"midtone","name":"Midtone"}
PATCH /auth/api/tenants/midtone             {"config":{"app_url":"https://beta.midtone.tech","allowed_origins":["https://admin.midtone.tech"]}}
PUT   /auth/api/tenants/midtone/connectors/google   {"client_id":"…","client_secret":"…"}
POST  /auth/api/tenants/midtone/roles       {"name":"admin","scopes":[]}
PUT   /auth/api/tenants/midtone/users/{id}/roles    {"role_ids":["…"]}
```

Register `{app_url}/api/auth/social/{provider}/callback` as the redirect URI with Google and GitHub. `PATCH` replaces the whole `config`, so send every field you want to keep.

## Development

```sh
cp .env.example .env   # DATABASE_URL, PUBLIC_URL, ADMIN_EMAIL/ADMIN_PASSWORD…
cargo run              # :4112; without SMTP_URL, mails (with their links) are logged
cargo test             # needs DATABASE_URL to a Postgres the tests can create databases on
```
