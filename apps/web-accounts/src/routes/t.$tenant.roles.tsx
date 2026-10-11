import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, FormDialog } from '@/components/form-dialog'
import { Empty, Loaded, Page } from '@/components/page'
import { Badge } from '@/components/ui/badge'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { none, Role, useList, useSend } from '@/lib/api'

export const Route = createFileRoute('/t/$tenant/roles')({
  component: Roles,
})

const body = (d: FormData) => ({
  name: d.get('name'),
  description: d.get('description'),
  scopes: String(d.get('scopes')).split(/\s+/).filter(Boolean),
})

function RoleFields({ role }: { role?: Role }) {
  return (
    <>
      <Field label="Name" name="name" defaultValue={role?.name} required />
      <Field label="Description" name="description" defaultValue={role?.description} />
      <Field
        label="Scopes"
        name="scopes"
        defaultValue={role?.scopes.join(' ')}
        placeholder="posts:read posts:write"
        hint="Space separated. A user's access token carries the scopes of their roles that the application requested."
      />
    </>
  )
}

function Roles() {
  const { tenant } = Route.useParams()
  const path = `/tenants/${tenant}/roles`
  const roles = useList(z.array(Role), path)
  const send = useSend()
  return (
    <Page
      title="Roles"
      actions={
        <FormDialog trigger="New role" submit="Create" onSubmit={(d) => send(Role, path, 'POST', body(d))}>
          <RoleFields />
        </FormDialog>
      }
    >
      <Loaded query={roles}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty>No roles yet. Roles grant scopes to the users they are assigned to.</Empty>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead>Scopes</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r) => (
                  <TableRow key={r.id}>
                    <TableCell className="font-medium">{r.name}</TableCell>
                    <TableCell>{r.description}</TableCell>
                    <TableCell className="space-x-1">
                      {r.scopes.map((s) => (
                        <Badge key={s} variant="secondary">
                          {s}
                        </Badge>
                      ))}
                    </TableCell>
                    <TableCell className="space-x-2 text-right">
                      <FormDialog
                        trigger="Edit"
                        variant="outline"
                        title={`Edit ${r.name}`}
                        onSubmit={(d) => send(Role, `${path}/${r.id}`, 'PATCH', body(d))}
                      >
                        <RoleFields role={r} />
                      </FormDialog>
                      <FormDialog
                        trigger="Delete"
                        destructive
                        title={`Delete ${r.name}`}
                        submit="Delete"
                        description="Users holding this role lose its scopes at their next token refresh."
                        onSubmit={() => send(none, `${path}/${r.id}`, 'DELETE')}
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
