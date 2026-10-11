import { useQuery } from '@tanstack/react-query'
import { createRootRoute, Link, Outlet, useLocation, useParams } from '@tanstack/react-router'
import { AppWindow, Building2, ChevronsUpDown, LogOut, Plug, Settings, Shield, Users } from 'lucide-react'
import { z } from 'zod'
import { ThemeToggle } from '@/components/theme-toggle'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from '@/components/ui/sidebar'
import { Tenant, useList } from '@/lib/api'
import { accessToken, userManager } from '@/lib/auth'

export const Route = createRootRoute({
  // Everything except the OIDC callback needs a signed-in operator.
  beforeLoad: async ({ location }) => {
    if (location.pathname !== '/callback') await accessToken()
  },
  component: Root,
})

const sections = [
  ['/t/$tenant/users', 'Users', Users],
  ['/t/$tenant/roles', 'Roles', Shield],
  ['/t/$tenant/organizations', 'Organizations', Building2],
  ['/t/$tenant/applications', 'Applications', AppWindow],
  ['/t/$tenant/connectors', 'Connectors', Plug],
  ['/t/$tenant/settings', 'Settings', Settings],
] as const

function Root() {
  const { pathname } = useLocation()
  if (pathname === '/callback') return <Outlet />
  return (
    <SidebarProvider>
      <Nav />
      <SidebarInset className="min-w-0">
        <header className="flex items-center gap-2 border-b p-2">
          <SidebarTrigger />
          <ThemeToggle />
        </header>
        <div className="flex-1 p-4">
          <Outlet />
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}

function Nav() {
  const { tenant } = useParams({ strict: false })
  const tenants = useList(z.array(Tenant), '/tenants')
  const operator = useQuery({ queryKey: ['operator'], queryFn: () => userManager.getUser() })
  const { setOpenMobile } = useSidebar()
  // The drawer does not close itself on client-side navigation.
  const close = () => setOpenMobile(false)
  const current = tenants.data?.find((t) => t.slug === tenant)

  return (
    <Sidebar variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger render={<SidebarMenuButton size="lg" aria-label="Switch tenant" />}>
                <div className="grid flex-1 text-left leading-tight">
                  <span className="text-xs text-muted-foreground">Tenant</span>
                  <span className="truncate font-medium">{current?.name ?? tenant ?? 'All tenants'}</span>
                </div>
                <ChevronsUpDown className="ml-auto" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="min-w-56">
                {tenants.data?.map((t) => (
                  <DropdownMenuItem
                    key={t.id}
                    render={<Link to="/t/$tenant/users" params={{ tenant: t.slug }} onClick={close} />}
                  >
                    {t.name}
                  </DropdownMenuItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem render={<Link to="/" onClick={close} />}>All tenants</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        {tenant && (
          <SidebarGroup>
            <SidebarGroupLabel>Manage</SidebarGroupLabel>
            <SidebarMenu>
              {sections.map(([to, label, Icon]) => (
                <SidebarMenuItem key={to}>
                  <SidebarMenuButton
                    render={<Link to={to} params={{ tenant }} onClick={close} className="[&.active]:font-semibold" />}
                  >
                    <Icon />
                    {label}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroup>
        )}
      </SidebarContent>
      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton onClick={() => userManager.signoutRedirect()}>
              <LogOut />
              <span className="truncate">Sign out {operator.data?.profile.email}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  )
}
