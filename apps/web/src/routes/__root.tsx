import { createRootRoute, Link, Outlet } from '@tanstack/react-router'

export const Route = createRootRoute({
  component: () => (
    <>
      <nav className="flex gap-4 border-b p-4">
        <Link to="/" className="[&.active]:font-semibold">
          Home
        </Link>
        <Link to="/about" className="[&.active]:font-semibold">
          About
        </Link>
        <Link to="/location" className="[&.active]:font-semibold">
          Location
        </Link>
      </nav>
      <main className="p-4">
        <Outlet />
      </main>
    </>
  ),
})
