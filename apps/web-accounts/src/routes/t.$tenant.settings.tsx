import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, Form, FormDialog, Toggle } from '@/components/form-dialog'
import { Loaded, Page, Section } from '@/components/page'
import { Badge } from '@/components/ui/badge'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Key, none, Tenant, useList, useSend } from '@/lib/api'

export const Route = createFileRoute('/t/$tenant/settings')({
  component: SettingsPage,
})

function SettingsPage() {
  const { tenant } = Route.useParams()
  const path = `/tenants/${tenant}`
  const settings = useList(Tenant, path)
  const keys = useList(z.array(Key), `${path}/keys`)
  const send = useSend()
  const navigate = useNavigate()
  return (
    <Loaded query={settings}>
      {(t) => (
        <Page title="Settings">
          <Section title="Tenant">
            <Form onSubmit={(d) => send(Tenant, path, 'PATCH', { name: d.get('name') })}>
              <Field label="Name" name="name" defaultValue={t.name} required hint={`Shown on the hosted sign-in pages. Slug: ${t.slug}`} />
            </Form>
          </Section>

          <Section title="Sign-in">
            <Form
              onSubmit={(d) =>
                send(Tenant, path, 'PATCH', {
                  config: {
                    sign_up_enabled: d.get('sign_up_enabled') === 'on',
                    password_min_length: Number(d.get('password_min_length')),
                    session_ttl_days: Number(d.get('session_ttl_days')),
                    access_token_ttl_secs: Number(d.get('access_token_ttl_secs')),
                    refresh_token_ttl_days: Number(d.get('refresh_token_ttl_days')),
                  },
                })
              }
            >
              <Toggle
                label="Allow self sign-up (otherwise only operators create users)"
                name="sign_up_enabled"
                defaultChecked={t.config.sign_up_enabled}
              />
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Minimum password length" name="password_min_length" type="number" min={8} max={128} defaultValue={t.config.password_min_length} required />
                <Field label="Browser session lifetime (days)" name="session_ttl_days" type="number" min={1} max={365} defaultValue={t.config.session_ttl_days} required />
                <Field label="Access token lifetime (seconds)" name="access_token_ttl_secs" type="number" min={60} max={86400} defaultValue={t.config.access_token_ttl_secs} required />
                <Field label="Refresh token lifetime (days)" name="refresh_token_ttl_days" type="number" min={1} max={365} defaultValue={t.config.refresh_token_ttl_days} required />
              </div>
            </Form>
          </Section>

          <Section
            title="Signing keys"
            actions={
              <FormDialog
                trigger="Rotate key"
                variant="outline"
                submit="Rotate"
                description="Creates a new signing key and retires the current one at once. Every access and ID token of this tenant stops verifying; applications recover with their refresh token. Use it when a key may have leaked."
                onSubmit={() => send(none, `${path}/keys`, 'POST')}
              />
            }
          >
            <Loaded query={keys}>
              {(rows) => (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Key ID</TableHead>
                      <TableHead>Created</TableHead>
                      <TableHead>Status</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {rows.map((k) => (
                      <TableRow key={k.kid}>
                        <TableCell>
                          <code>{k.kid}</code>
                        </TableCell>
                        <TableCell>{new Date(k.created_at).toLocaleString()}</TableCell>
                        <TableCell>
                          <Badge variant={k.retired_at ? 'outline' : 'secondary'}>{k.retired_at ? 'Retired' : 'Active'}</Badge>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </Loaded>
          </Section>

          {/* the admin tenant is what this console signs in to */}
          {t.slug !== 'admin' && (
            <Section
              title="Delete tenant"
              actions={
                <FormDialog
                  trigger="Delete tenant"
                  destructive
                  title={`Delete ${t.name}`}
                  submit="Delete everything"
                  description="Deletes every user, role, organization, application and key of this tenant. This cannot be undone."
                  onSubmit={async () => {
                    await send(none, path, 'DELETE')
                    await navigate({ to: '/' })
                  }}
                />
              }
            >
              <p className="text-sm text-muted-foreground">Removes the tenant and all of its data permanently.</p>
            </Section>
          )}
        </Page>
      )}
    </Loaded>
  )
}
