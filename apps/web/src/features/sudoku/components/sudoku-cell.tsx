import { cn } from "@/lib/utils"

/** Thick borders on the 3x3 box seams; thin elsewhere. */
export function boxBorders(row: number, col: number): string {
  return cn(
    "border-border/60 border-t border-l",
    col === 8 && "border-r",
    row === 8 && "border-b",
    col % 3 === 0 && "border-l-2 border-l-foreground/40",
    row % 3 === 0 && "border-t-2 border-t-foreground/40",
    col === 8 && "border-r-2 border-r-foreground/40",
    row === 8 && "border-b-2 border-b-foreground/40",
  )
}

interface CellProps {
  row: number
  col: number
  value: number
  given: boolean
  selected?: boolean
  conflict?: boolean
  interactive?: boolean
  onSelect?: (row: number, col: number) => void
}

/** A single interactive cell on the player's own board. */
export function SudokuCell({
  row,
  col,
  value,
  given,
  selected,
  conflict,
  interactive,
  onSelect,
}: CellProps) {
  return (
    <button
      type="button"
      disabled={!interactive || given}
      onClick={() => onSelect?.(row, col)}
      aria-label={`Row ${row + 1}, column ${col + 1}${value ? `, ${value}` : ", empty"}`}
      className={cn(
        "flex aspect-square items-center justify-center text-base font-medium tabular-nums transition-colors sm:text-lg",
        boxBorders(row, col),
        given ? "bg-muted/60 text-foreground" : "text-primary",
        interactive && !given && "cursor-pointer hover:bg-accent",
        selected && "bg-primary/15 ring-primary ring-2 ring-inset",
        conflict && !given && "bg-destructive/15 text-destructive",
      )}
    >
      {value !== 0 ? value : ""}
    </button>
  )
}
