import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, Form, FormDialog, Toggle } from '@/components/form-dialog'
import { Loaded, Page, Section } from '@/components/page'
import { Connector, none, useList, useSend } from '@/lib/api'
import { AUTH_URL } from '@/lib/auth'

export const Route = createFileRoute('/t/$tenant/connectors')({
  component: Connectors,
})

const providers = [
  ['google', 'Google'],
  ['github', 'GitHub'],
] as const

function Connectors() {
  const { tenant } = Route.useParams()
  const path = `/tenants/${tenant}/connectors`
  const connectors = useList(z.array(Connector), path)
  const send = useSend()
  return (
    <Page title="Connectors">
      <p className="text-sm text-muted-foreground">
        Social sign-in. Create an OAuth app with the provider, register the callback URL shown below there, then paste its
        credentials here. Accounts are linked by email, and only when the provider reports the address as verified.
      </p>
      <Loaded query={connectors}>
        {(rows) =>
          providers.map(([id, label]) => {
            const current = rows.find((c) => c.provider === id)
            return (
              <Section
                key={id}
                title={label}
                actions={
                  current && (
                    <FormDialog
                      trigger="Remove"
                      destructive
                      title={`Remove ${label} sign-in`}
                      submit="Remove"
                      description="Users who only sign in with this provider will need a password reset to get back in."
                      onSubmit={() => send(none, `${path}/${id}`, 'DELETE')}
                    />
                  )
                }
              >
                <p className="text-sm text-muted-foreground">
                  Callback URL: <code className="break-all text-foreground">{`${AUTH_URL}/auth/t/${tenant}/social/${id}/callback`}</code>
                </p>
                <Form
                  // remount when the connector appears or disappears, so the fields show the stored state
                  key={String(!!current)}
                  onSubmit={(d) =>
                    send(Connector, `${path}/${id}`, 'PUT', {
                      client_id: d.get('client_id'),
                      client_secret: d.get('client_secret') || undefined,
                      enabled: d.get('enabled') === 'on',
                    })
                  }
                >
                  <Field label="Client ID" name="client_id" defaultValue={current?.client_id} required />
                  <Field
                    label="Client secret"
                    name="client_secret"
                    type="password"
                    autoComplete="off"
                    required={!current}
                    hint={current ? 'Stored. Leave blank to keep it; it is never shown again.' : undefined}
                  />
                  <Toggle label="Enabled" name="enabled" defaultChecked={current?.enabled ?? true} />
                </Form>
              </Section>
            )
          })
        }
      </Loaded>
    </Page>
  )
}
