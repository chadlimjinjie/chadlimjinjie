import { createFileRoute } from '@tanstack/react-router'
import { ConnectFourLanding } from '@/features/connect-four/components/connect-four-page'

export const Route = createFileRoute('/connect-four/')({
  component: ConnectFourLanding,
})
