import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { z } from 'zod'
import { Field, FormDialog } from '@/components/form-dialog'
import { Loaded, Page } from '@/components/page'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { date, Tenant, useList, useSend } from '@/lib/api'

export const Route = createFileRoute('/')({
  component: Tenants,
})

function Tenants() {
  const tenants = useList(z.array(Tenant), '/tenants')
  const send = useSend()
  const navigate = useNavigate()
  return (
    <Page
      title="Tenants"
      actions={
        <FormDialog
          trigger="New tenant"
          submit="Create"
          description="A tenant is an isolated set of users, roles, applications and sign-in settings."
          onSubmit={async (d) => {
            const created = await send(Tenant, '/tenants', 'POST', { slug: d.get('slug'), name: d.get('name') })
            await navigate({ to: '/t/$tenant/users', params: { tenant: created.slug } })
          }}
        >
          <Field label="Name" name="name" required />
          <Field
            label="Slug"
            name="slug"
            required
            pattern="[a-z0-9][a-z0-9-]{0,62}"
            hint="Lowercase letters, digits and dashes. Part of every URL of this tenant and cannot be changed."
          />
        </FormDialog>
      }
    >
      <Loaded query={tenants}>
        {(rows) => (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Slug</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((t) => (
                <TableRow key={t.id}>
                  <TableCell>
                    <Link to="/t/$tenant/users" params={{ tenant: t.slug }} className="font-medium underline-offset-4 hover:underline">
                      {t.name}
                    </Link>
                  </TableCell>
                  <TableCell>
                    <code>{t.slug}</code>
                  </TableCell>
                  <TableCell>{date(t.created_at)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Loaded>
    </Page>
  )
}
