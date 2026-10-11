// Anonymous per-browser identity for the games: a stable uuid kept in
// localStorage so a refresh reconnects to the same seat.

const PID_KEY = 'playerId'
const NAME_KEY = 'playerName'

export function getOrCreatePlayerId(): string {
  let id = localStorage.getItem(PID_KEY)
  if (!id) {
    id = crypto.randomUUID()
    localStorage.setItem(PID_KEY, id)
  }
  return id
}

export function getPlayerName(): string {
  return localStorage.getItem(NAME_KEY) ?? ''
}

export function setPlayerName(name: string): void {
  localStorage.setItem(NAME_KEY, name.trim())
}
