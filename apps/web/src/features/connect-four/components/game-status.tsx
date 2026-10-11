import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { ConnectFourState } from "../hooks/use-connect-four"
import type { Seat } from "../types"

function SeatDot({ seat }: { seat: Seat }) {
  return (
    <span
      className={cn("inline-block size-3 rounded-full", seat === "red" ? "bg-red-500" : "bg-yellow-400")}
      aria-hidden
    />
  )
}

export function GameStatus({
  state,
  onRematch,
}: {
  state: ConnectFourState
  onRematch: () => void
}) {
  const { status, seat, turn, winner, players, opponentConnected, rematchOfferedByOpponent } = state
  const mySeat = seat === "red" || seat === "yellow" ? seat : null
  const gameOver = status === "won" || status === "draw"

  let message: string
  if (status === "waiting") {
    message = "Waiting for an opponent to join…"
  } else if (status === "won") {
    message = winner === mySeat ? "You win! 🎉" : "You lost."
  } else if (status === "draw") {
    message = "It's a draw."
  } else {
    message = turn === mySeat ? "Your turn" : `${players[turn]?.name ?? "Opponent"}'s turn`
  }

  return (
    <div className="flex flex-col items-center gap-3">
      <div className="flex items-center gap-2 text-base font-medium">
        {status === "playing" && <SeatDot seat={turn} />}
        <span>{message}</span>
      </div>

      {status === "playing" && mySeat && !opponentConnected && (
        <p className="text-sm text-muted-foreground">
          Opponent disconnected — waiting for them to return…
        </p>
      )}

      {gameOver && mySeat && (
        <div className="flex flex-col items-center gap-1">
          <Button onClick={onRematch}>{rematchOfferedByOpponent ? "Accept rematch" : "Rematch"}</Button>
          {rematchOfferedByOpponent && (
            <p className="text-xs text-muted-foreground">Opponent wants a rematch</p>
          )}
        </div>
      )}
    </div>
  )
}
