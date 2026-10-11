import { useCallback, useEffect, useMemo, useState } from "react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SIZE } from "../types"
import { useSudoku, type SudokuMode } from "../hooks/use-sudoku"
import { getPlayerName, setPlayerName } from "@/lib/player"
import { CountdownTimer } from "./countdown-timer"
import { CopyLink } from "@/components/copy-link"
import { GameStatus } from "./game-status"
import { NumberPad } from "./number-pad"
import { OpponentBoard } from "./opponent-board"
import { SudokuBoard } from "./sudoku-board"

export function SudokuRoom({ mode }: { mode: SudokuMode }) {
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

function Room({ mode, name }: { mode: SudokuMode; name: string }) {
  const { state, setCell, clearCell, rematch } = useSudoku(mode, name)
  const [selected, setSelected] = useState<{ row: number; col: number } | null>(null)

  // Make the room URL shareable without remounting (keeps the socket alive).
  useEffect(() => {
    if (state.code) {
      const target = `/sudoku/${state.code}`
      if (window.location.pathname !== target) {
        window.history.replaceState(null, "", target)
      }
    }
  }, [state.code])

  const interactive = state.status === "playing"
  const bothPresent = Boolean(state.players.p1 && state.players.p2)

  const isGiven = useCallback(
    (row: number, col: number) => state.givens[row]?.[col] !== 0,
    [state.givens],
  )

  const onInput = useCallback(
    (value: number) => {
      if (!interactive || !selected || isGiven(selected.row, selected.col)) return
      setCell(selected.row, selected.col, value)
    },
    [interactive, selected, isGiven, setCell],
  )

  const onErase = useCallback(() => {
    if (!interactive || !selected || isGiven(selected.row, selected.col)) return
    clearCell(selected.row, selected.col)
  }, [interactive, selected, isGiven, clearCell])

  const onMove = useCallback((dr: number, dc: number) => {
    setSelected((prev) => {
      const base = prev ?? { row: 0, col: 0 }
      return {
        row: Math.min(SIZE - 1, Math.max(0, base.row + dr)),
        col: Math.min(SIZE - 1, Math.max(0, base.col + dc)),
      }
    })
  }, [])

  const shareUrl = useMemo(() => {
    if (!state.code || typeof window === "undefined") return ""
    return `${window.location.origin}/sudoku/${state.code}`
  }, [state.code])

  return (
    <div className="flex flex-1 flex-col items-center gap-6 px-4 py-8">
      <div className="flex flex-col items-center gap-1">
        <h1 className="text-xl font-semibold tracking-tight">Sudoku Duel</h1>
        <span className="rounded-full bg-secondary px-2 py-0.5 text-xs font-medium text-secondary-foreground capitalize">
          {state.difficulty}
        </span>
      </div>

      {(state.status === "playing" || state.status === "over") && (
        <CountdownTimer deadline={state.deadline} active={state.status === "playing"} />
      )}

      <div className="grid w-full max-w-5xl gap-8 md:grid-cols-2">
        <div className="flex flex-col items-center gap-3">
          <BoardHeader
            label={`You${state.seat ? ` · ${state.players[state.seat]?.name ?? name}` : ""}`}
            correct={state.myCorrectCount}
          />
          <SudokuBoard
            board={state.myBoard}
            givens={state.givens}
            interactive={interactive}
            selected={selected}
            onSelect={(row, col) => setSelected({ row, col })}
            onInput={onInput}
            onErase={onErase}
            onMove={onMove}
          />
          <NumberPad disabled={!interactive} onInput={onInput} onErase={onErase} />
        </div>

        <div className="flex flex-col items-center gap-3">
          <BoardHeader label="Opponent" correct={state.oppCorrectCount} />
          <OpponentBoard givens={state.givens} filled={state.oppFilledMask} />
          <p className="text-muted-foreground text-xs">
            You can watch them fill cells, but not what they type.
          </p>
        </div>
      </div>

      <GameStatus state={state} onRematch={rematch} />

      {state.error && (
        <p role="alert" className="text-destructive text-sm">
          {state.error.message}
        </p>
      )}

      {state.status === "waiting" && shareUrl && !bothPresent && (
        <div className="flex flex-col items-center gap-2">
          <p className="text-muted-foreground text-sm">Share this link with a friend to play:</p>
          <CopyLink url={shareUrl} />
        </div>
      )}

      {state.connection !== "open" && (
        <p className="text-muted-foreground text-xs">Reconnecting…</p>
      )}
    </div>
  )
}

function BoardHeader({ label, correct }: { label: string; correct: number }) {
  return (
    <div className="flex w-full max-w-[min(90vw,26rem)] items-center justify-between">
      <span className="truncate text-sm font-medium">{label}</span>
      <span className="text-muted-foreground text-sm tabular-nums">{correct}/81 correct</span>
    </div>
  )
}

function NameGate({ onSubmit }: { onSubmit: (name: string) => void }) {
  const [value, setValue] = useState("")
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4 px-4">
      <div className="w-full max-w-sm space-y-3 text-center">
        <h1 className="text-xl font-semibold tracking-tight">Sudoku Duel</h1>
        <p className="text-muted-foreground text-sm">Choose a display name to join the game.</p>
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
