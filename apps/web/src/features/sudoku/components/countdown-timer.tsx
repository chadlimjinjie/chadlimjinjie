import { useEffect, useState } from "react"

import { cn } from "@/lib/utils"

const FULL_MS = 300_000 // 5:00 nominal, for the progress bar scale

interface CountdownTimerProps {
  deadline: number | null
  active: boolean
}

/** Live mm:ss countdown to `deadline` (unix ms) with a progress bar. */
export function CountdownTimer({ deadline, active }: CountdownTimerProps) {
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (!active || deadline === null) return
    const id = setInterval(() => setNow(Date.now()), 250)
    return () => clearInterval(id)
  }, [active, deadline])

  const remaining = deadline !== null ? Math.max(0, deadline - now) : FULL_MS
  const totalSeconds = Math.ceil(remaining / 1000)
  const mm = Math.floor(totalSeconds / 60)
  const ss = totalSeconds % 60
  const low = active && remaining <= 30_000

  return (
    <div className="flex w-full max-w-xs flex-col items-center gap-1.5">
      <span
        className={cn(
          "text-2xl font-semibold tabular-nums",
          low ? "text-destructive" : "text-foreground",
        )}
      >
        {mm}:{ss.toString().padStart(2, "0")}
      </span>
      <progress
        aria-label="Time remaining"
        value={active ? remaining : FULL_MS}
        max={FULL_MS}
        className="h-1.5 w-full accent-primary"
      />
    </div>
  )
}
