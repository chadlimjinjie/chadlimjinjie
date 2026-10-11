import { useEffect, useMemo, useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"

import { Board } from "./board"
import { GameStatus } from "./game-status"
import { CopyLink } from "@/components/copy-link"
import { useConnectFour, type ConnectFourMode, type ConnectFourState } from "../hooks/use-connect-four"
import { getPlayerName, setPlayerName } from "@/lib/player"
import type { PlayerInfo, Seat } from "../types"

export function ConnectFourRoom({ mode }: { mode: ConnectFourMode }) {
  const [name, setName] = useState(getPlayerName)

  if (name === "") {
    return (
      <NameGate
        onSubmit={(n) => {
          setPlayerName(n)
          setName(n)
        }}
      />
    )
  }
  return <Room mode={mode} name={name} />
}

function Room({ mode, name }: { mode: ConnectFourMode; name: string }) {
  const { state, playColumn, rematch } = useConnectFour(mode, name)

  // Once the room code is known (created or matched), make the URL shareable
  // without remounting — that keeps the live socket intact.
  useEffect(() => {
    if (state.code) {
      const target = `/connect-four/${state.code}`
      if (window.location.pathname !== target) {
        window.history.replaceState(null, "", target)
      }
    }
  }, [state.code])

  const mySeat = state.seat === "red" || state.seat === "yellow" ? state.seat : null
  const interactive = state.status === "playing" && mySeat !== null && state.turn === mySeat
  const bothPresent = Boolean(state.players.red && state.players.yellow)

  const shareUrl = useMemo(() => {
    if (!state.code || typeof window === "undefined") return ""
    return `${window.location.origin}/connect-four/${state.code}`
  }, [state.code])

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-4 py-8">
      <Header state={state} mySeat={mySeat} />

      <Board
        board={state.board}
        winningCells={state.winningCells}
        interactive={interactive}
        onPlay={playColumn}
      />

      <GameStatus state={state} onRematch={rematch} />

      {state.error && (
        <p role="alert" className="text-sm text-destructive">
          {state.error.message}
        </p>
      )}

      {state.status === "waiting" && shareUrl && !bothPresent && (
        <div className="flex flex-col items-center gap-2">
          <p className="text-sm text-muted-foreground">Share this link with a friend to play:</p>
          <CopyLink url={shareUrl} />
        </div>
      )}

      {state.connection !== "open" && <p className="text-xs text-muted-foreground">Reconnecting…</p>}
    </div>
  )
}

function Header({ state, mySeat }: { state: ConnectFourState; mySeat: Seat | null }) {
  return (
    <div className="flex flex-col items-center gap-2">
      <h1 className="text-xl font-semibold tracking-tight">Connect Four</h1>
      <div className="flex items-center gap-4 text-sm">
        <SeatChip seat="red" info={state.players.red} you={mySeat === "red"} />
        <span className="text-muted-foreground">vs</span>
        <SeatChip seat="yellow" info={state.players.yellow} you={mySeat === "yellow"} />
      </div>
    </div>
  )
}

function SeatChip({ seat, info, you }: { seat: Seat; info?: PlayerInfo; you: boolean }) {
  return (
    <span className="flex items-center gap-1.5">
      <span
        className={cn("inline-block size-3 rounded-full", seat === "red" ? "bg-red-500" : "bg-yellow-400")}
        aria-hidden
      />
      <span className={cn(!info && "text-muted-foreground", info && !info.connected && "line-through opacity-60")}>
        {info?.name ?? "Waiting…"}
        {you && " (you)"}
      </span>
    </span>
  )
}

function NameGate({ onSubmit }: { onSubmit: (name: string) => void }) {
  const [value, setValue] = useState("")
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4 px-4">
      <div className="w-full max-w-sm space-y-3 text-center">
        <h1 className="text-xl font-semibold tracking-tight">Connect Four</h1>
        <p className="text-sm text-muted-foreground">Choose a display name to join the game.</p>
        <form
          onSubmit={(e) => {
            e.preventDefault()
            if (value.trim()) onSubmit(value.trim())
          }}
          className="flex gap-2"
        >
          <Input
            autoFocus
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="Your name"
            maxLength={24}
          />
          <Button type="submit" disabled={!value.trim()}>
            Join
          </Button>
        </form>
      </div>
    </div>
  )
}
