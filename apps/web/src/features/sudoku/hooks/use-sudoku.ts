import { useCallback, useEffect, useReducer, useRef } from "react"

import { buildWsUrl } from "@/lib/ws-url"
import { emptyGrid } from "../game-logic"
import {
  ServerMessageSchema,
  type ClientMessage,
  type Difficulty,
  type Grid,
  type Identity,
  type Mask,
  type PlayerInfo,
  type Seat,
  type ServerMessage,
  type Status,
  type Winner,
} from "../types"

/** How the connection bootstraps once the socket opens. */
export type SudokuMode =
  | { kind: "create"; difficulty: Difficulty }
  | { kind: "join"; code: string }
  | { kind: "find"; difficulty: Difficulty }

export interface SudokuState {
  connection: "connecting" | "open" | "closed"
  seat: Seat | null
  code: string | null
  you: Identity | null
  status: Status
  difficulty: Difficulty
  givens: Grid
  myBoard: Grid
  myCorrectCount: number
  oppFilledMask: Mask
  oppCorrectCount: number
  deadline: number | null
  winner: Winner | null
  players: Partial<Record<Seat, PlayerInfo>>
  opponentConnected: boolean
  rematchOfferedByOpponent: boolean
  matchCode: string | null
  error: { code: string; message: string } | null
}

const initialState: SudokuState = {
  connection: "connecting",
  seat: null,
  code: null,
  you: null,
  status: "waiting",
  difficulty: "medium",
  givens: emptyGrid(),
  myBoard: emptyGrid(),
  myCorrectCount: 0,
  oppFilledMask: emptyMask(),
  oppCorrectCount: 0,
  deadline: null,
  winner: null,
  players: {},
  opponentConnected: false,
  rematchOfferedByOpponent: false,
  matchCode: null,
  error: null,
}

function emptyMask(): Mask {
  return Array.from({ length: 9 }, () => Array.from({ length: 9 }, () => false))
}

type Action =
  | { kind: "connection"; value: SudokuState["connection"] }
  | { kind: "message"; msg: ServerMessage }
  | { kind: "optimistic"; row: number; col: number; value: number }

function reducer(state: SudokuState, action: Action): SudokuState {
  if (action.kind === "connection") {
    return { ...state, connection: action.value }
  }
  if (action.kind === "optimistic") {
    // Apply the local edit immediately for input latency; the next server
    // `state` snapshot reconciles (and is authoritative).
    const myBoard = state.myBoard.map((r) => r.slice())
    if (myBoard[action.row]) myBoard[action.row][action.col] = action.value
    return { ...state, myBoard }
  }
  const msg = action.msg
  switch (msg.type) {
    case "joined":
      return { ...state, seat: msg.seat, code: msg.code, you: msg.you, error: null }
    case "state": {
      const opponentSeat: Seat | null = state.seat
        ? state.seat === "p1"
          ? "p2"
          : "p1"
        : null
      const opponent = opponentSeat ? msg.players[opponentSeat] : undefined
      return {
        ...state,
        status: msg.status,
        difficulty: msg.difficulty,
        givens: msg.givens,
        myBoard: msg.board,
        myCorrectCount: msg.yourCorrect,
        oppFilledMask: msg.oppFilled,
        oppCorrectCount: msg.oppCorrect,
        deadline: msg.deadline,
        winner: msg.winner ?? (msg.status === "over" ? state.winner : null),
        players: msg.players as Partial<Record<Seat, PlayerInfo>>,
        opponentConnected: opponent?.connected ?? false,
        rematchOfferedByOpponent: msg.status === "playing" ? false : state.rematchOfferedByOpponent,
      }
    }
    case "match_found":
      return { ...state, matchCode: msg.code }
    case "game_over":
      return { ...state, status: "over", winner: msg.winner }
    case "opponent_disconnected":
      return { ...state, opponentConnected: false }
    case "opponent_reconnected":
      return { ...state, opponentConnected: true }
    case "rematch_offered":
      return { ...state, rematchOfferedByOpponent: msg.by !== state.seat }
    case "rematch_started":
      return { ...state, rematchOfferedByOpponent: false, winner: null }
    case "error":
      return { ...state, error: { code: msg.code, message: msg.message } }
    case "pong":
      return state
    default:
      return state
  }
}

/**
 * Opens a live game connection and drives a Sudoku match. The same socket is
 * reused across create -> join and find -> join transitions, and reconnects
 * with backoff (re-joining the established room) so a dropped connection or a
 * refresh re-attaches to the same seat. Mirrors `use-connect-four`.
 */
export function useSudoku(mode: SudokuMode, name: string) {
  const [state, dispatch] = useReducer(reducer, initialState)

  const wsRef = useRef<WebSocket | null>(null)
  const currentCodeRef = useRef<string | null>(null)
  const nameRef = useRef(name)
  const intentionalCloseRef = useRef(false)
  const reconnectAttemptsRef = useRef(0)
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

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
        ws.send(JSON.stringify({ type: "join", code, name: nameRef.current }))
      } else if (mode.kind === "create") {
        ws.send(JSON.stringify({ type: "create", difficulty: mode.difficulty, name: nameRef.current }))
      } else if (mode.kind === "join") {
        currentCodeRef.current = mode.code
        ws.send(JSON.stringify({ type: "join", code: mode.code, name: nameRef.current }))
      } else {
        ws.send(JSON.stringify({ type: "find_match", name: nameRef.current, difficulty: mode.difficulty }))
      }
    }

    const connect = async () => {
      dispatch({ kind: "connection", value: "connecting" })
      let url: string
      try {
        url = buildWsUrl("/sudoku/ws")
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
        if (msg.type === "joined") currentCodeRef.current = msg.code
        if (msg.type === "match_found") {
          currentCodeRef.current = msg.code
          ws.send(JSON.stringify({ type: "join", code: msg.code, name: nameRef.current }))
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
        if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: "leave" }))
        ws.close()
      }
      wsRef.current = null
    }
    // mode is stable per mount (route-derived); reconnection handles the rest.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode.kind, mode.kind === "join" ? mode.code : null])

  const setCell = useCallback(
    (row: number, col: number, value: number) => {
      dispatch({ kind: "optimistic", row, col, value })
      send({ type: "set_cell", row, col, value })
    },
    [send],
  )
  const clearCell = useCallback(
    (row: number, col: number) => {
      dispatch({ kind: "optimistic", row, col, value: 0 })
      send({ type: "clear_cell", row, col })
    },
    [send],
  )
  const rematch = useCallback(() => send({ type: "rematch" }), [send])

  return { state, setCell, clearCell, rematch }
}

function safeJson(data: unknown): unknown {
  if (typeof data !== "string") return null
  try {
    return JSON.parse(data)
  } catch {
    return null
  }
}
