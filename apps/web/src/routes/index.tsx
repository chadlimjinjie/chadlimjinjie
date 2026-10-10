import { createFileRoute } from '@tanstack/react-router'
import { Button } from '@/components/ui/button'

export const Route = createFileRoute('/')({
  component: () => (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Home</h1>
      <Button>Click me</Button>
    </div>
  ),
})
