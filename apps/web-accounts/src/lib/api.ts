import { useQuery, useQueryClient } from '@tanstack/react-query'
import { z } from 'zod'
import { accessToken, AUTH_URL, userManager } from './auth'

export const Tenant = z.object({
  id: z.string(),
  slug: z.string(),
  name: z.string(),
  created_at: z.string(),
  config: z.object({
    sign_up_enabled: z.boolean(),
    password_min_length: z.number(),
    session_ttl_days: z.number(),
    access_token_ttl_secs: z.number(),
    refresh_token_ttl_days: z.number(),
    login_ui_url: z.string().nullish(),
  }),
})

export const User = z.object({
  id: z.string(),
  email: z.string(),
  name: z.string(),
  email_verified: z.boolean(),
  suspended: z.boolean(),
  created_at: z.string(),
})

export const UserDetail = User.extend({
  has_password: z.boolean(),
  role_ids: z.array(z.string()),
  providers: z.array(z.string()),
  sessions: z.array(
    z.object({
      id: z.string(),
      ip: z.string().nullable(),
      user_agent: z.string().nullable(),
      created_at: z.string(),
      expires_at: z.string(),
    }),
  ),
})

export const Role = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  scopes: z.array(z.string()),
})
export type Role = z.infer<typeof Role>

export const Org = z.object({ id: z.string(), name: z.string(), created_at: z.string(), members: z.number() })
export type Org = z.infer<typeof Org>

export const Member = z.object({ user_id: z.string(), email: z.string(), name: z.string(), role: z.string() })

export const App = z.object({
  id: z.string(),
  client_id: z.string(),
  name: z.string(),
  confidential: z.boolean(),
  redirect_uris: z.array(z.string()),
  post_logout_uris: z.array(z.string()),
  created_at: z.string(),
  // present only in the response that creates the client
  client_secret: z.string().nullish(),
})
export type App = z.infer<typeof App>

export const Secret = z.object({ client_secret: z.string() })

export const Connector = z.object({ provider: z.string(), client_id: z.string(), enabled: z.boolean() })

export const Key = z.object({ kid: z.string(), created_at: z.string(), retired_at: z.string().nullable() })

/** For responses with no body worth reading. */
export const none = z.unknown()

export async function api<T>(schema: z.ZodType<T>, path: string, method = 'GET', body?: unknown): Promise<T> {
  const res = await fetch(`${AUTH_URL}/auth/api${path}`, {
    method,
    headers: {
      authorization: `Bearer ${await accessToken()}`,
      ...(body !== undefined && { 'content-type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (res.status === 401) {
    // The token was revoked server-side (key rotation, suspension): start over.
    await userManager.removeUser()
    await accessToken()
  }
  if (!res.ok) {
    const problem = await res.json().catch(() => null)
    throw new Error(problem?.error ?? res.statusText)
  }
  return schema.parse(res.status === 204 ? undefined : await res.json())
}

export function useList<T>(schema: z.ZodType<T>, path: string) {
  return useQuery({ queryKey: [path], queryFn: () => api(schema, path) })
}

/** A write to the management API. Every write refreshes every list: the console is small, and stale rows are worse than a refetch. */
export function useSend() {
  const queryClient = useQueryClient()
  return async <T,>(schema: z.ZodType<T>, path: string, method: string, body?: unknown) => {
    const out = await api(schema, path, method, body)
    void queryClient.invalidateQueries()
    return out
  }
}

export const date = (iso: string) => new Date(iso).toLocaleDateString()
