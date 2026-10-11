import { useEffect } from 'react'
import { createRootRoute, Link, Outlet, useMatches } from '@tanstack/react-router'
import { ChevronDown } from 'lucide-react'
import { ThemeToggle } from '@/components/theme-toggle'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from '@/components/ui/sidebar'
import { pages } from '@/lib/seo'

export const Route = createRootRoute({
  component: Root,
})

const links = [
  ['/about', 'About'],
  ['/location', 'Location'],
  ['/coins', 'Coins'],
] as const
const games = [
  ['/sudoku', 'Sudoku'],
  ['/connect-four', 'Connect Four'],
] as const
const socials = [
  ['GitHub', 'https://github.com/chadlimjinjie'],
  ['LinkedIn', 'https://www.linkedin.com/in/chadlimjinjie'],
  ['X', 'https://x.com/chadlimjinjie'],
  ['Email', 'mailto:chadlimjinjie@gmail.com'],
]
const year = new Date().getFullYear()

function Root() {
  const path = useMatches({ select: (matches) => matches.at(-1)?.fullPath })
  // The static head tags come from the build (vite.config.ts); this keeps them right after client navigation.
  useEffect(() => {
    const page = pages[path ?? '/'] ?? pages['/']
    document.title = page.title
    document
      .querySelector('meta[name="description"]')
      ?.setAttribute('content', page.description)
  }, [path])
  const inGame = /^\/(sudoku|connect-four)/.test(path ?? '')

  return (
    // open={false} pins the desktop sidebar shut; the mobile drawer has its own state.
    <SidebarProvider open={false}>
      <MobileNav inGame={inGame} />
      <div className="flex min-h-svh min-w-0 flex-1 flex-col">
        <nav className="flex items-center gap-4 border-b p-4">
          <SidebarTrigger className="md:hidden" />
          <Link to="/" aria-label="Home" className="hidden md:block">
            <img src="/favicon.svg" alt="" className="size-6 transition-transform hover:rotate-12" />
          </Link>
          <div className="hidden md:contents">
            {links.map(([to, label]) => (
              <Link key={to} to={to} className="[&.active]:font-semibold">
                {label}
              </Link>
            ))}
            <DropdownMenu>
              <DropdownMenuTrigger className={`flex items-center gap-1 ${inGame ? 'font-semibold' : ''}`}>
                Games
                <ChevronDown className="size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                {games.map(([to, label]) => (
                  <DropdownMenuItem key={to} render={<Link to={to} className="[&.active]:font-semibold" />}>
                    {label}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <ThemeToggle />
        </nav>
        <main className="flex-1 p-4">
          <Outlet />
        </main>
        <footer className="flex flex-wrap items-center justify-between gap-4 border-t p-4 text-sm text-muted-foreground">
          <p>© {year} Chad Lim</p>
          <ul className="flex gap-4">
            {socials.map(([label, href]) => (
              <li key={label}>
                <a
                  href={href}
                  // mailto in a new tab leaves a blank tab behind in some browsers
                  target={href.startsWith('http') ? '_blank' : undefined}
                  rel="noreferrer"
                  className="hover:text-foreground"
                >
                  {label}
                </a>
              </li>
            ))}
          </ul>
        </footer>
      </div>
    </SidebarProvider>
  )
}

function MobileNav({ inGame }: { inGame: boolean }) {
  const { setOpenMobile } = useSidebar()
  // The drawer does not close itself on client-side navigation.
  const close = () => setOpenMobile(false)
  return (
    <Sidebar>
      <SidebarHeader className="p-4">
        <Link to="/" aria-label="Home" onClick={close} className="self-start">
          <img src="/favicon.svg" alt="" className="size-6 transition-transform hover:rotate-12" />
        </Link>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarMenu>
            {links.map(([to, label]) => (
              <SidebarMenuItem key={to}>
                <SidebarMenuButton
                  render={<Link to={to} onClick={close} className="[&.active]:font-semibold" />}
                >
                  {label}
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
            <Collapsible defaultOpen={inGame} className="group/collapsible" render={<SidebarMenuItem />}>
              <CollapsibleTrigger render={<SidebarMenuButton />}>
                Games
                <ChevronDown className="ml-auto transition-transform group-data-open/collapsible:rotate-180" />
              </CollapsibleTrigger>
              <CollapsibleContent>
                <SidebarMenuSub>
                  {games.map(([to, label]) => (
                    <SidebarMenuSubItem key={to}>
                      <SidebarMenuSubButton
                        render={<Link to={to} onClick={close} className="[&.active]:font-semibold" />}
                      >
                        {label}
                      </SidebarMenuSubButton>
                    </SidebarMenuSubItem>
                  ))}
                </SidebarMenuSub>
              </CollapsibleContent>
            </Collapsible>
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  )
}
