import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, Form, FormDialog, Toggle } from '@/components/form-dialog'
import { Empty, Loaded, Page, Section } from '@/components/page'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { date, none, Role, User, UserDetail, useList, useSend } from '@/lib/api'

export const Route = createFileRoute('/t/$tenant/users/$id')({
  component: UserPage,
})

function UserPage() {
  const { tenant, id } = Route.useParams()
  const path = `/tenants/${tenant}/users/${id}`
  const user = useList(UserDetail, path)
  const roles = useList(z.array(Role), `/tenants/${tenant}/roles`)
  const send = useSend()
  const navigate = useNavigate()
  return (
    <Loaded query={user}>
      {(u) => (
        <Page
          title={u.email}
          actions={
            <>
              <FormDialog
                trigger="Set password"
                variant="outline"
                description="Signs the user out of every session."
                onSubmit={(d) => send(User, path, 'PATCH', { password: d.get('password') })}
              >
                <Field label="New password" name="password" type="password" autoComplete="new-password" required />
              </FormDialog>
              <FormDialog
                trigger="Delete"
                destructive
                title="Delete user"
                submit="Delete"
                description={`${u.email} and everything linked to the account will be removed. This cannot be undone.`}
                onSubmit={async () => {
                  await send(none, path, 'DELETE')
                  await navigate({ to: '/t/$tenant/users', params: { tenant } })
                }}
              />
            </>
          }
        >
          <Section title="Profile">
            <Form
              onSubmit={(d) =>
                send(User, path, 'PATCH', {
                  email: d.get('email'),
                  name: d.get('name'),
                  email_verified: d.get('email_verified') === 'on',
                  suspended: d.get('suspended') === 'on',
                })
              }
            >
              <Field label="Email" name="email" type="email" defaultValue={u.email} required />
              <Field label="Name" name="name" defaultValue={u.name} />
              <Toggle label="Email verified" name="email_verified" defaultChecked={u.email_verified} />
              <Toggle label="Suspended (cannot sign in; existing sessions are ended)" name="suspended" defaultChecked={u.suspended} />
            </Form>
            <p className="text-sm text-muted-foreground">
              Signs in with: {[u.has_password && 'password', ...u.providers].filter(Boolean).join(', ') || 'nothing yet'} ·
              Created {date(u.created_at)}
            </p>
          </Section>

          <Section title="Roles">
            <Loaded query={roles}>
              {(all) =>
                all.length === 0 ? (
                  <Empty>This tenant has no roles yet.</Empty>
                ) : (
                  <Form onSubmit={(d) => send(none, `${path}/roles`, 'PUT', { role_ids: d.getAll('role') })}>
                    {all.map((r) => (
                      <Label key={r.id} className="flex items-center gap-2">
                        <Checkbox name="role" value={r.id} defaultChecked={u.role_ids.includes(r.id)} />
                        {r.name}
                        <span className="font-normal text-muted-foreground">{r.scopes.join(' ')}</span>
                      </Label>
                    ))}
                  </Form>
                )
              }
            </Loaded>
          </Section>

          <Section
            title="Sessions"
            actions={
              <FormDialog
                trigger="Sign out everywhere"
                variant="outline"
                submit="Sign out"
                description="Ends every browser session and revokes every refresh token of this user."
                onSubmit={() => send(none, `${path}/sessions`, 'DELETE')}
              />
            }
          >
            {u.sessions.length === 0 ? (
              <Empty>No active sessions.</Empty>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Started</TableHead>
                    <TableHead>Address</TableHead>
                    <TableHead>Browser</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {u.sessions.map((s) => (
                    <TableRow key={s.id}>
                      <TableCell>{new Date(s.created_at).toLocaleString()}</TableCell>
                      <TableCell>{s.ip}</TableCell>
                      <TableCell className="max-w-64 truncate">{s.user_agent}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </Section>
        </Page>
      )}
    </Loaded>
  )
}
