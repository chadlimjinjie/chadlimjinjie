import { useCallback, useEffect, useReducer, useRef } from "react"

import { buildWsUrl } from "@/lib/ws-url"
import { emptyBoard } from "../game-logic"
import {
  ServerMessageSchema,
  type Board,
  type ClientMessage,
  type Identity,
  type PlayerInfo,
  type Seat,
  type ServerMessage,
  type Status,
} from "../types"

/** How the connection bootstraps once the socket opens. */
export type ConnectFourMode =
  | { kind: "create" }
  | { kind: "join"; code: string }
  | { kind: "find" }

export interface ConnectFourState {
  connection: "connecting" | "open" | "closed"
  seat: Seat | "spectator" | null
  code: string | null
  you: Identity | null
  board: Board
  turn: Seat
  status: Status
  winner: Seat | null
  winningCells: [number, number][]
  players: Partial<Record<Seat, PlayerInfo>>
  opponentConnected: boolean
  rematchOfferedByOpponent: boolean
  matchCode: string | null
  error: { code: string; message: string } | null
}

const initialState: ConnectFourState = {
  connection: "connecting",
  seat: null,
  code: null,
  you: null,
  board: emptyBoard(),
  turn: "red",
  status: "waiting",
  winner: null,
  winningCells: [],
  players: {},
  opponentConnected: false,
  rematchOfferedByOpponent: false,
  matchCode: null,
  error: null,
}

type Action =
  | { kind: "connection"; value: ConnectFourState["connection"] }
  | { kind: "message"; msg: ServerMessage }

function reducer(state: ConnectFourState, action: Action): ConnectFourState {
  if (action.kind === "connection") {
    return { ...state, connection: action.value }
  }
  const msg = action.msg
  switch (msg.type) {
    case "joined":
      return { ...state, seat: msg.seat, code: msg.code, you: msg.you, error: null }
    case "state": {
      const mySeat = state.seat === "red" || state.seat === "yellow" ? state.seat : null
      const opponentSeat: Seat | null = mySeat ? (mySeat === "red" ? "yellow" : "red") : null
      const opponent = opponentSeat ? msg.players[opponentSeat] : undefined
      return {
        ...state,
        board: msg.board,
        turn: msg.turn,
        status: msg.status,
        winner: msg.winner ?? null,
        winningCells: msg.winningCells ?? [],
        players: msg.players as Partial<Record<Seat, PlayerInfo>>,
        opponentConnected: opponent?.connected ?? false,
        // A fresh status of "playing" means a new/rematched game — clear offers.
        rematchOfferedByOpponent: msg.status === "playing" ? false : state.rematchOfferedByOpponent,
      }
    }
    case "match_found":
      return { ...state, matchCode: msg.code }
    case "opponent_disconnected":
      return { ...state, opponentConnected: false }
    case "opponent_reconnected":
      return { ...state, opponentConnected: true }
    case "rematch_offered":
      return { ...state, rematchOfferedByOpponent: msg.by !== state.seat }
    case "rematch_started":
      return { ...state, rematchOfferedByOpponent: false, winner: null, winningCells: [] }
    case "error":
      return { ...state, error: { code: msg.code, message: msg.message } }
    case "pong":
      return state
    default:
      return state
  }
}

/**
 * Opens a live game connection and drives a Connect Four game. The same socket
 * is reused across create -> join and find -> join transitions, and reconnects
 * with backoff (re-joining the established room) so a dropped connection or a
 * refresh re-attaches to the same seat.
 */
export function useConnectFour(mode: ConnectFourMode, name: string) {
  const [state, dispatch] = useReducer(reducer, initialState)

  const wsRef = useRef<WebSocket | null>(null)
  const currentCodeRef = useRef<string | null>(null) // room to (re)join once known
  const nameRef = useRef(name)
  const intentionalCloseRef = useRef(false)
  const reconnectAttemptsRef = useRef(0)
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Keep the latest name available to the long-lived socket callbacks without
  // re-running the connection effect when it changes.
  useEffect(() => {
    nameRef.current = name
  }, [name])

  const send = useCallback((msg: ClientMessage) => {
    const ws = wsRef.current
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(msg))
  }, [])

  useEffect(() => {
    intentionalCloseRef.current = false
    let cancelled = false

    const initialSend = (ws: WebSocket) => {
      const code = currentCodeRef.current
      if (code) {
        ws.send(JSON.stringify({ type: "join", code, name: nameRef.current } satisfies ClientMessage))
      } else if (mode.kind === "create") {
        ws.send(JSON.stringify({ type: "create", name: nameRef.current } satisfies ClientMessage))
      } else if (mode.kind === "join") {
        currentCodeRef.current = mode.code
        ws.send(JSON.stringify({ type: "join", code: mode.code, name: nameRef.current } satisfies ClientMessage))
      } else {
        ws.send(JSON.stringify({ type: "find_match", name: nameRef.current } satisfies ClientMessage))
      }
    }

    const connect = async () => {
      dispatch({ kind: "connection", value: "connecting" })
      let url: string
      try {
        url = buildWsUrl("/connect-four/ws")
      } catch {
        if (!cancelled) scheduleReconnect()
        return
      }
      if (cancelled) return

      const ws = new WebSocket(url)
      wsRef.current = ws

      ws.onopen = () => {
        reconnectAttemptsRef.current = 0
        dispatch({ kind: "connection", value: "open" })
        initialSend(ws)
      }

      ws.onmessage = (event) => {
        const parsed = ServerMessageSchema.safeParse(safeJson(event.data))
        if (!parsed.success) return
        const msg = parsed.data
        // Remember the room so reconnects re-join it.
        if (msg.type === "joined") currentCodeRef.current = msg.code
        // A queue match resolves into joining that room on this same socket.
        if (msg.type === "match_found") {
          currentCodeRef.current = msg.code
          ws.send(JSON.stringify({ type: "join", code: msg.code, name: nameRef.current } satisfies ClientMessage))
        }
        dispatch({ kind: "message", msg })
      }

      ws.onclose = () => {
        dispatch({ kind: "connection", value: "closed" })
        if (!intentionalCloseRef.current && !cancelled) scheduleReconnect()
      }

      ws.onerror = () => ws.close()
    }

    const scheduleReconnect = () => {
      const attempt = reconnectAttemptsRef.current++
      const delay = Math.min(1000 * 2 ** attempt, 10_000)
      reconnectTimerRef.current = setTimeout(connect, delay)
    }

    connect()

    return () => {
      cancelled = true
      intentionalCloseRef.current = true
      if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current)
      const ws = wsRef.current
      if (ws) {
        if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: "leave" } satisfies ClientMessage))
        ws.close()
      }
      wsRef.current = null
    }
    // mode is stable per mount (route-derived); reconnection handles the rest.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode.kind, mode.kind === "join" ? mode.code : null])

  const playColumn = useCallback((column: number) => send({ type: "move", column }), [send])
  const rematch = useCallback(() => send({ type: "rematch" }), [send])

  return { state, playColumn, rematch }
}

function safeJson(data: unknown): unknown {
  if (typeof data !== "string") return null
  try {
    return JSON.parse(data)
  } catch {
    return null
  }
}
