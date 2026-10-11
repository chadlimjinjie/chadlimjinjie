import { createFileRoute, redirect } from '@tanstack/react-router'
import { userManager } from '@/lib/auth'

export const Route = createFileRoute('/callback')({
  // In beforeLoad rather than an effect: the authorization code is single use, and StrictMode runs effects twice.
  beforeLoad: async () => {
    const user = await userManager.signinRedirectCallback()
    // state is the path the operator was on; anything that is not a local path falls back to home
    const path = typeof user.state === 'string' && /^\/(?!\/)/.test(user.state) ? user.state : '/'
    throw redirect({ to: path })
  },
  errorComponent: ({ error }) => (
    <main className="mx-auto grid max-w-sm gap-3 p-8">
      <h1 className="text-xl font-semibold">Sign-in failed</h1>
      <p role="alert" className="text-sm text-destructive">
        {error instanceof Error ? error.message : String(error)}
      </p>
      <a href="/" className="underline">
        Try again
      </a>
    </main>
  ),
})
