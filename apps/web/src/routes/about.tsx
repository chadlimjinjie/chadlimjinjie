import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/about')({
  component: () => <h1 className="text-2xl font-semibold">About</h1>,
})
