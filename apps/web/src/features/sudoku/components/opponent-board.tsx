import { cn } from "@/lib/utils"
import type { Grid, Mask } from "../types"
import { boxBorders } from "./sudoku-cell"

interface OpponentBoardProps {
  givens: Grid
  filled: Mask
}

/**
 * Read-only view of the opponent's board. Shows the shared clue digits and,
 * where the opponent has entered *something*, a filled marker — but NEVER the
 * digit they typed. This is the visible face of the server's redaction: the
 * client only ever receives a boolean `filled` mask for the opponent.
 */
export function OpponentBoard({ givens, filled }: OpponentBoardProps) {
  return (
    <div className="border-foreground/40 grid w-full max-w-[min(90vw,26rem)] grid-cols-9 border-2 bg-card select-none">
      {givens.map((rowArr, r) =>
        rowArr.map((given, c) => {
          const isGiven = given !== 0
          const isFilled = filled[r]?.[c] === true
          return (
            <div
              key={`${r}-${c}`}
              className={cn(
                "flex aspect-square items-center justify-center text-base font-medium tabular-nums sm:text-lg",
                boxBorders(r, c),
                isGiven && "bg-muted/60 text-foreground",
              )}
              aria-label={
                isGiven
                  ? `Row ${r + 1}, column ${c + 1}, clue ${given}`
                  : isFilled
                    ? `Row ${r + 1}, column ${c + 1}, filled`
                    : `Row ${r + 1}, column ${c + 1}, empty`
              }
            >
              {isGiven ? (
                given
              ) : isFilled ? (
                <span className="bg-primary/70 size-2 rounded-full sm:size-2.5" aria-hidden />
              ) : (
                ""
              )}
            </div>
          )
        }),
      )}
    </div>
  )
}
