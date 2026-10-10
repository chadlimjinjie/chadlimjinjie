import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'

type Point = { lat: number; lon: number; timestamp: string } | null

// half-width of the map view in degrees (~500 m)
const SPAN = 0.005

export const Route = createFileRoute('/location')({
  component: Location,
})

function Location() {
  const { data: point, isPending, error } = useQuery({
    queryKey: ['location'],
    queryFn: async (): Promise<Point> => {
      const res = await fetch(`${import.meta.env.VITE_API_URL}/api/location`)
      if (!res.ok) throw new Error(`Location request failed: ${res.status}`)
      return res.json()
    },
    // matches the function's s-maxage, polling faster only hits the CDN cache
    refetchInterval: 30_000,
  })
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">Location</h1>
      {isPending ? (
        <p>Loading…</p>
      ) : error ? (
        <p role="alert">{error.message}</p>
      ) : point ? (
        <>
          <p>
            Updated{' '}
            <time dateTime={point.timestamp}>
              {new Date(point.timestamp).toLocaleString()}
            </time>
          </p>
          {/* ponytail: static OSM embed, position fixed per page load. Switch to Leaflet for a live-moving marker or custom styling. */}
          <iframe
            title="Map of current location"
            className="h-96 w-full rounded border"
            src={`https://www.openstreetmap.org/export/embed.html?bbox=${point.lon - SPAN},${point.lat - SPAN},${point.lon + SPAN},${point.lat + SPAN}&layer=mapnik&marker=${point.lat},${point.lon}`}
          />
        </>
      ) : (
        <p>No location yet.</p>
      )}
    </div>
  )
}
