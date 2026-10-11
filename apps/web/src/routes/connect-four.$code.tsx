import { createFileRoute } from '@tanstack/react-router'
import { ConnectFourRoom } from '@/features/connect-four/components/connect-four-room'
import type { ConnectFourMode } from '@/features/connect-four/hooks/use-connect-four'

export const Route = createFileRoute('/connect-four/$code')({
  component: Room,
})

function Room() {
  const { code } = Route.useParams()
  // `new` and `find` are reserved pseudo-codes for the create / random-match flows.
  const lower = code.toLowerCase()
  const mode: ConnectFourMode =
    lower === 'new'
      ? { kind: 'create' }
      : lower === 'find'
        ? { kind: 'find' }
        : { kind: 'join', code: code.toUpperCase() }
  return <ConnectFourRoom mode={mode} />
}
