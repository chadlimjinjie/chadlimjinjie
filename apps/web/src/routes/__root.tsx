import { useEffect } from 'react'
import { createRootRoute, Link, Outlet, useMatches } from '@tanstack/react-router'
import { ThemeToggle } from '@/components/theme-toggle'
import { pages } from '@/lib/seo'

export const Route = createRootRoute({
  component: Root,
})

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
    <>
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
        <ThemeToggle />
      </nav>
      <main className="p-4">
        <Outlet />
      </main>
    </>
  )
}
