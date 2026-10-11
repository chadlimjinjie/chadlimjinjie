import { useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, FormDialog, Toggle } from '@/components/form-dialog'
import { Empty, Loaded, Page } from '@/components/page'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { date, User, useList, useSend } from '@/lib/api'

export const Route = createFileRoute('/t/$tenant/users/')({
  component: Users,
})

function Users() {
  const { tenant } = Route.useParams()
  const [search, setSearch] = useState('')
  const users = useList(z.array(User), `/tenants/${tenant}/users?q=${encodeURIComponent(search)}`)
  const send = useSend()
  return (
    <Page
      title="Users"
      actions={
        <FormDialog
          trigger="New user"
          submit="Create"
          onSubmit={(d) =>
            send(User, `/tenants/${tenant}/users`, 'POST', {
              email: d.get('email'),
              name: d.get('name'),
              password: d.get('password') || undefined,
              email_verified: d.get('email_verified') === 'on',
            })
          }
        >
          <Field label="Email" name="email" type="email" required />
          <Field label="Name" name="name" />
          <Field
            label="Password"
            name="password"
            type="password"
            autoComplete="new-password"
            hint="Optional. Without one, the user sets it through “Forgot password”."
          />
          <Toggle label="Mark email as verified" name="email_verified" />
        </FormDialog>
      }
    >
      <Input
        type="search"
        aria-label="Search users"
        placeholder="Search by email or name"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        className="max-w-xs"
      />
      <Loaded query={users}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty>No users found.</Empty>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Email</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Created</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((u) => (
                  <TableRow key={u.id}>
                    <TableCell>
                      <Link
                        to="/t/$tenant/users/$id"
                        params={{ tenant, id: u.id }}
                        className="font-medium underline-offset-4 hover:underline"
                      >
                        {u.email}
                      </Link>
                    </TableCell>
                    <TableCell>{u.name}</TableCell>
                    <TableCell className="space-x-1">
                      {u.suspended && <Badge variant="destructive">Suspended</Badge>}
                      {!u.email_verified && <Badge variant="outline">Unverified</Badge>}
                      {!u.suspended && u.email_verified && <Badge variant="secondary">Active</Badge>}
                    </TableCell>
                    <TableCell>{date(u.created_at)}</TableCell>
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
