import { useEffect } from "react"

import { cn } from "@/lib/utils"
import { conflicts } from "../game-logic"
import type { Grid } from "../types"
import { SudokuCell } from "./sudoku-cell"

interface SudokuBoardProps {
  board: Grid
  givens: Grid
  interactive: boolean
  selected: { row: number; col: number } | null
  onSelect: (row: number, col: number) => void
  onInput: (value: number) => void
  onErase: () => void
  onMove: (dr: number, dc: number) => void
}

/** The player's own interactive 9x9 board, with keyboard input and conflict hints. */
export function SudokuBoard({
  board,
  givens,
  interactive,
  selected,
  onSelect,
  onInput,
  onErase,
  onMove,
}: SudokuBoardProps) {
  const bad = conflicts(board)

  // Keyboard: digits enter values, Backspace/Delete/0 erase, arrows move.
  useEffect(() => {
    if (!interactive) return
    const handler = (e: KeyboardEvent) => {
      if (e.key >= "1" && e.key <= "9") {
        onInput(Number(e.key))
      } else if (e.key === "Backspace" || e.key === "Delete" || e.key === "0") {
        onErase()
      } else if (e.key === "ArrowUp") {
        e.preventDefault()
        onMove(-1, 0)
      } else if (e.key === "ArrowDown") {
        e.preventDefault()
        onMove(1, 0)
      } else if (e.key === "ArrowLeft") {
        e.preventDefault()
        onMove(0, -1)
      } else if (e.key === "ArrowRight") {
        e.preventDefault()
        onMove(0, 1)
      }
    }
    window.addEventListener("keydown", handler)
    return () => window.removeEventListener("keydown", handler)
  }, [interactive, onInput, onErase, onMove])

  return (
    <div
      className={cn(
        "border-foreground/40 grid w-full max-w-[min(90vw,26rem)] grid-cols-9 border-2 bg-card select-none",
        !interactive && "opacity-95",
      )}
    >
      {board.map((rowArr, r) =>
        rowArr.map((value, c) => (
          <SudokuCell
            key={`${r}-${c}`}
            row={r}
            col={c}
            value={value}
            given={givens[r]?.[c] !== 0}
            selected={selected?.row === r && selected?.col === c}
            conflict={bad.has(`${r},${c}`)}
            interactive={interactive}
            onSelect={onSelect}
          />
        )),
      )}
    </div>
  )
}
