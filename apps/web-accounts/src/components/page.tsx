import type { ReactNode } from 'react'
import type { UseQueryResult } from '@tanstack/react-query'
import { Skeleton } from '@/components/ui/skeleton'

export function Page({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className="mx-auto grid max-w-5xl gap-6">
      <header className="flex flex-wrap items-center gap-2">
        <h1 className="text-xl font-semibold break-all">{title}</h1>
        <div className="ml-auto flex flex-wrap gap-2">{actions}</div>
      </header>
      {children}
    </div>
  )
}

export function Section({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="grid gap-4 rounded-xl border p-4">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="font-medium">{title}</h2>
        <div className="ml-auto flex flex-wrap gap-2">{actions}</div>
      </div>
      {children}
    </section>
  )
}

/** Renders a query's data, or its loading and error states. */
export function Loaded<T>({ query, children }: { query: UseQueryResult<T>; children: (data: T) => ReactNode }) {
  if (query.error) {
    return (
      <p role="alert" className="text-sm text-destructive">
        {query.error.message}
      </p>
    )
  }
  if (query.data === undefined) return <Skeleton className="h-32" />
  return children(query.data)
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground">{children}</p>
}
