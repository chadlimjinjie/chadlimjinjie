import { getOrCreatePlayerId } from './player'

/** WebSocket URL of a game server under `VITE_API_URL`, e.g. `/connect-four/ws` or `/sudoku/ws`. */
export function buildWsUrl(path: string): string {
  const url = new URL(path, import.meta.env.VITE_API_URL)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  url.searchParams.set('pid', getOrCreatePlayerId())
  return url.toString()
}
