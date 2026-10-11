import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, Form, FormDialog } from '@/components/form-dialog'
import { Empty, Loaded, Page } from '@/components/page'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Member, none, Org, User, useList, useSend } from '@/lib/api'

export const Route = createFileRoute('/t/$tenant/organizations')({
  component: Organizations,
})

function Organizations() {
  const { tenant } = Route.useParams()
  const path = `/tenants/${tenant}/organizations`
  const orgs = useList(z.array(Org), path)
  const send = useSend()
  return (
    <Page
      title="Organizations"
      actions={
        <FormDialog trigger="New organization" submit="Create" onSubmit={(d) => send(Org, path, 'POST', { name: d.get('name') })}>
          <Field label="Name" name="name" required />
        </FormDialog>
      }
    >
      <Loaded query={orgs}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty>No organizations yet. Organizations group this tenant's users into teams or customers.</Empty>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Members</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((o) => (
                  <TableRow key={o.id}>
                    <TableCell className="font-medium">{o.name}</TableCell>
                    <TableCell>{o.members}</TableCell>
                    <TableCell className="space-x-2 text-right">
                      <Dialog>
                        <DialogTrigger render={<Button variant="outline" size="sm" />}>Members</DialogTrigger>
                        <DialogContent>
                          <DialogHeader>
                            <DialogTitle>{o.name}</DialogTitle>
                            <DialogDescription>
                              Members and their role in this organization, as issued in the organizations claim.
                            </DialogDescription>
                          </DialogHeader>
                          <Members tenant={tenant} org={o} />
                        </DialogContent>
                      </Dialog>
                      <FormDialog
                        trigger="Rename"
                        variant="outline"
                        onSubmit={(d) => send(Org, `${path}/${o.id}`, 'PATCH', { name: d.get('name') })}
                      >
                        <Field label="Name" name="name" defaultValue={o.name} required />
                      </FormDialog>
                      <FormDialog
                        trigger="Delete"
                        destructive
                        title={`Delete ${o.name}`}
                        submit="Delete"
                        description="Removes the organization and its memberships. The users themselves are kept."
                        onSubmit={() => send(none, `${path}/${o.id}`, 'DELETE')}
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

// Mounted only while its dialog is open, so the member list is fetched on demand.
function Members({ tenant, org }: { tenant: string; org: Org }) {
  const path = `/tenants/${tenant}/organizations/${org.id}/members`
  const members = useList(z.array(Member), path)
  const users = useList(z.array(User), `/tenants/${tenant}/users?q=`)
  const send = useSend()
  return (
    <div className="grid gap-4">
      <Loaded query={members}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty>No members yet.</Empty>
          ) : (
            <ul className="grid gap-2">
              {rows.map((m) => (
                <li key={m.user_id} className="flex items-center gap-2 text-sm">
                  <span className="min-w-0 flex-1 truncate">{m.email}</span>
                  <span className="text-muted-foreground">{m.role}</span>
                  <Button
                    variant="ghost"
                    size="sm"
                    aria-label={`Remove ${m.email}`}
                    onClick={() => send(none, `${path}/${m.user_id}`, 'DELETE')}
                  >
                    Remove
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Loaded>
      <Form submit="Add or update member" onSubmit={(d) => send(none, `${path}/${d.get('user')}`, 'PUT', { role: d.get('role') })}>
        <div className="grid gap-1.5">
          <Label htmlFor="member-user">User</Label>
          <NativeSelect id="member-user" name="user" required>
            {users.data?.map((u) => (
              <NativeSelectOption key={u.id} value={u.id}>
                {u.email}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <Field label="Role" name="role" defaultValue="member" required />
      </Form>
    </div>
  )
}
