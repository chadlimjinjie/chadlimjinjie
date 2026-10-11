import { useId, useState } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, FormDialog, Toggle } from '@/components/form-dialog'
import { Empty, Loaded, Page } from '@/components/page'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Textarea } from '@/components/ui/textarea'
import { App, none, Secret, useList, useSend } from '@/lib/api'
import { AUTH_URL } from '@/lib/auth'

export const Route = createFileRoute('/t/$tenant/applications')({
  component: Applications,
})

const lines = (value: FormDataEntryValue | null) => String(value ?? '').split(/\s+/).filter(Boolean)

function Uris({ label, name, value, hint }: { label: string; name: string; value?: string[]; hint: string }) {
  const id = useId()
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Textarea id={id} name={name} defaultValue={value?.join('\n')} rows={2} aria-describedby={`${id}-hint`} />
      <p id={`${id}-hint`} className="text-xs text-muted-foreground">
        {hint}
      </p>
    </div>
  )
}

function AppFields({ app }: { app?: App }) {
  return (
    <>
      <Field label="Name" name="name" defaultValue={app?.name} required />
      <Uris label="Redirect URIs" name="redirect_uris" value={app?.redirect_uris} hint="One per line. Matched exactly." />
      <Uris
        label="Post sign-out redirect URIs"
        name="post_logout_uris"
        value={app?.post_logout_uris}
        hint="One per line. Where a user may land after signing out."
      />
    </>
  )
}

function Applications() {
  const { tenant } = Route.useParams()
  const path = `/tenants/${tenant}/applications`
  const apps = useList(z.array(App), path)
  const send = useSend()
  // A client secret exists in readable form only in the response that created it.
  const [secret, setSecret] = useState<{ name: string; value: string }>()
  const issuer = `${AUTH_URL}/auth/t/${tenant}`
  return (
    <Page
      title="Applications"
      actions={
        <FormDialog
          trigger="New application"
          submit="Create"
          onSubmit={async (d) => {
            const app = await send(App, path, 'POST', {
              name: d.get('name'),
              confidential: d.get('confidential') === 'on',
              redirect_uris: lines(d.get('redirect_uris')),
              post_logout_uris: lines(d.get('post_logout_uris')),
            })
            if (app.client_secret) setSecret({ name: app.name, value: app.client_secret })
          }}
        >
          <AppFields />
          <Toggle label="Confidential (server-side app that can keep a client secret)" name="confidential" />
        </FormDialog>
      }
    >
      <p className="text-sm text-muted-foreground">
        OIDC issuer: <code className="break-all text-foreground">{issuer}</code>. Clients discover the endpoints from{' '}
        <code>/.well-known/openid-configuration</code> and must use the authorization code flow with PKCE.
      </p>
      {secret && (
        <div role="status" className="flex flex-wrap items-center gap-2 rounded-xl border p-4 text-sm">
          <span>
            Client secret for {secret.name}, shown only once: <code className="break-all font-medium">{secret.value}</code>
          </span>
          <Button variant="outline" size="sm" className="ml-auto" onClick={() => setSecret(undefined)}>
            I have stored it
          </Button>
        </div>
      )}
      <Loaded query={apps}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty>No applications yet. Register one for each site or service that signs users in.</Empty>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Client ID</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((a) => (
                  <TableRow key={a.id}>
                    <TableCell className="font-medium">{a.name}</TableCell>
                    <TableCell>
                      <code>{a.client_id}</code>
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{a.confidential ? 'Confidential' : 'Public'}</Badge>
                    </TableCell>
                    <TableCell className="space-x-2 text-right">
                      <FormDialog
                        trigger="Edit"
                        variant="outline"
                        title={`Edit ${a.name}`}
                        onSubmit={(d) =>
                          send(App, `${path}/${a.id}`, 'PATCH', {
                            name: d.get('name'),
                            redirect_uris: lines(d.get('redirect_uris')),
                            post_logout_uris: lines(d.get('post_logout_uris')),
                          })
                        }
                      >
                        <AppFields app={a} />
                      </FormDialog>
                      {a.confidential && (
                        <FormDialog
                          trigger="New secret"
                          variant="outline"
                          title={`Replace the secret of ${a.name}`}
                          submit="Replace"
                          description="The current secret stops working immediately."
                          onSubmit={async () => {
                            const out = await send(Secret, `${path}/${a.id}/secret`, 'POST')
                            setSecret({ name: a.name, value: out.client_secret })
                          }}
                        />
                      )}
                      <FormDialog
                        trigger="Delete"
                        destructive
                        title={`Delete ${a.name}`}
                        submit="Delete"
                        description="Users can no longer sign in to this application, and its refresh tokens are removed."
                        onSubmit={() => send(none, `${path}/${a.id}`, 'DELETE')}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )
        }
      </Loaded>
    </Page>
  )
}
