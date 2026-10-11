import { UserManager } from 'oidc-client-ts'

/** Origin of the gateway that fronts auth-service. */
export const AUTH_URL = (import.meta.env.VITE_AUTH_URL || 'http://localhost:9080').replace(/\/$/, '')

// The console is an ordinary public OIDC client of the admin tenant (code + PKCE).
// Tokens live in sessionStorage (the library default), so they end with the tab.
export const userManager = new UserManager({
  authority: `${AUTH_URL}/auth/t/admin`,
  client_id: 'web-accounts',
  redirect_uri: `${location.origin}/callback`,
  post_logout_redirect_uri: location.origin,
  scope: 'openid profile email offline_access manage:all',
})

/** The operator's access token. With no usable session the browser is sent to the hosted sign-in page. */
export async function accessToken(): Promise<string> {
  let user = await userManager.getUser()
  if (user?.expired) user = await userManager.signinSilent().catch(() => null)
  if (user) return user.access_token
  await userManager.signinRedirect({ state: location.pathname })
  // The page is navigating away; never resolve, so nothing renders half-authenticated.
  return new Promise(() => {})
}
