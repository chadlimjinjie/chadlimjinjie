import { getOrCreatePlayerId } from './player'

/** WebSocket URL of a game server behind the APISIX gateway, e.g. `/ws` or `/sudoku/ws`. */
export function buildWsUrl(path: string): string {
  const url = new URL(path, import.meta.env.VITE_API_URL)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  url.searchParams.set('pid', getOrCreatePlayerId())
  return url.toString()
}
