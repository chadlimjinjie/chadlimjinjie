import { useEffect } from 'react'
import { createRootRoute, Link, Outlet, useMatches } from '@tanstack/react-router'
import { ThemeToggle } from '@/components/theme-toggle'
import { pages } from '@/lib/seo'

export const Route = createRootRoute({
  component: Root,
})

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

  return (
    <div className="flex min-h-svh flex-col">
      <nav className="flex items-center gap-4 border-b p-4">
        <Link to="/" aria-label="Home">
          <img src="/favicon.svg" alt="" className="size-6 transition-transform hover:rotate-12" />
        </Link>
        <Link to="/about" className="[&.active]:font-semibold">
          About
        </Link>
        <Link to="/location" className="[&.active]:font-semibold">
          Location
        </Link>
        <Link to="/coins" className="[&.active]:font-semibold">
          Coins
        </Link>
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
  )
}
