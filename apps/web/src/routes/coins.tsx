import { useState } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { Button } from '@/components/ui/button'
import { certUrl, coins } from '@/lib/coins'

const SERVICES = ['All', 'PCGS', 'NGC', 'ANACS'] as const

export const Route = createFileRoute('/coins')({
  component: Coins,
})

function Coins() {
  const [service, setService] = useState<(typeof SERVICES)[number]>('All')
  const [query, setQuery] = useState('')
  const q = query.trim().toLowerCase()
  const shown = coins.filter(
    (coin) =>
      (service === 'All' || coin.service === service) &&
      `${coin.name} ${coin.grade} ${coin.cert}`.toLowerCase().includes(q),
  )
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Coins</h1>
      <div className="flex flex-wrap items-center gap-2">
        {SERVICES.map((s) => (
          <Button
            key={s}
            variant={s === service ? 'default' : 'outline'}
            aria-pressed={s === service}
            onClick={() => setService(s)}
          >
            {s}
          </Button>
        ))}
        <input
          type="search"
          aria-label="Search coins"
          placeholder="Search name, grade or cert"
          className="h-8 min-w-0 flex-1 rounded-lg border bg-background px-2.5 text-sm"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <p aria-live="polite">
        {shown.length ? `${shown.length} coins` : 'No coins match.'}
      </p>
      <ul className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
        {shown.map((coin) => (
          <li key={`${coin.service}-${coin.cert}`}>
            <a
              href={certUrl(coin)}
              target="_blank"
              rel="noreferrer"
              className="block space-y-1 rounded-lg border p-2 hover:bg-muted"
            >
              {coin.images[0] ? (
                <img
                  src={coin.images[0]}
                  alt=""
                  loading="lazy"
                  referrerPolicy="no-referrer"
                  className="aspect-square w-full object-contain"
                />
              ) : (
                <div className="aspect-square w-full rounded bg-muted" />
              )}
              <p className="font-medium">{coin.name}</p>
              <p className="text-sm">{coin.grade}</p>
              <p className="text-sm text-muted-foreground">
                {coin.service} {coin.cert}
              </p>
            </a>
          </li>
        ))}
      </ul>
    </div>
  )
}
