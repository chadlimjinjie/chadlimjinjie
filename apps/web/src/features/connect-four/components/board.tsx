import { cn } from "@/lib/utils"
import { Cell } from "./cell"
import { COLS, ROWS, type Board as BoardType } from "../types"
import { isColumnFull } from "../game-logic"

interface BoardProps {
  board: BoardType
  winningCells: [number, number][]
  interactive: boolean
  onPlay: (col: number) => void
}

export function Board({ board, winningCells, interactive, onPlay }: BoardProps) {
  const winSet = new Set(winningCells.map(([r, c]) => `${r},${c}`))

  return (
    <div className="inline-grid grid-cols-7 gap-1.5 rounded-2xl bg-blue-600 p-2.5 shadow-lg sm:gap-2 sm:p-3 dark:bg-blue-700">
      {Array.from({ length: COLS }, (_, col) => {
        const canPlay = interactive && !isColumnFull(board, col)
        return (
          <button
            key={col}
            type="button"
            disabled={!canPlay}
            onClick={() => canPlay && onPlay(col)}
            aria-label={`Drop in column ${col + 1}`}
            className={cn(
              "flex flex-col gap-1.5 rounded-lg p-0.5 transition-colors sm:gap-2",
              canPlay ? "cursor-pointer hover:bg-white/10" : "cursor-default",
            )}
          >
            {Array.from({ length: ROWS }, (_, row) => (
              <Cell key={row} value={board[row]?.[col] ?? ""} winning={winSet.has(`${row},${col}`)} />
            ))}
          </button>
        )
      })}
    </div>
  )
}
