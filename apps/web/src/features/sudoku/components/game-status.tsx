import { Button } from "@/components/ui/button"
import type { SudokuState } from "../hooks/use-sudoku"

export function GameStatus({
  state,
  onRematch,
}: {
  state: SudokuState
  onRematch: () => void
}) {
  const { status, winner, opponentConnected, rematchOfferedByOpponent } = state
  const gameOver = status === "over"

  let message: string
  if (status === "waiting") {
    message = "Waiting for an opponent to join…"
  } else if (gameOver) {
    message = winner === "you" ? "You win! 🎉" : winner === "opponent" ? "You lost." : "It's a draw."
  } else {
    message = "Fill in as many correct cells as you can before time runs out!"
  }

  return (
    <div className="flex flex-col items-center gap-3">
      <span className="text-base font-medium">{message}</span>

      {status === "playing" && !opponentConnected && (
        <p className="text-muted-foreground text-sm">
          Opponent disconnected — waiting for them to return…
        </p>
      )}

      {gameOver && (
        <div className="flex flex-col items-center gap-1">
          <Button onClick={onRematch}>{rematchOfferedByOpponent ? "Accept rematch" : "Rematch"}</Button>
          {rematchOfferedByOpponent && (
            <p className="text-muted-foreground text-xs">Opponent wants a rematch</p>
          )}
        </div>
      )}
    </div>
  )
}
