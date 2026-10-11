import { z } from "zod"

/** Board dimensions — must match the Go server (internal/game/board.go). */
export const ROWS = 6
export const COLS = 7

/** A board square: empty, or occupied by a red/yellow disc. */
export const CellSchema = z.union([z.literal(""), z.literal("red"), z.literal("yellow")])
export type Cell = z.infer<typeof CellSchema>

/** A seated player's color. */
export type Seat = "red" | "yellow"

/** Game lifecycle. */
export type Status = "waiting" | "playing" | "won" | "draw"

/** Board is indexed [row][col]; row 0 is the top, row ROWS-1 the bottom. */
export const BoardSchema = z.array(z.array(CellSchema))
export type Board = z.infer<typeof BoardSchema>

export const IdentitySchema = z.object({
  kind: z.enum(["anon", "user"]),
  id: z.string(),
})
export type Identity = z.infer<typeof IdentitySchema>

export const PlayerInfoSchema = z.object({
  name: z.string(),
  connected: z.boolean(),
})
export type PlayerInfo = z.infer<typeof PlayerInfoSchema>

// ---- Server -> client messages ----------------------------------------------

const JoinedSchema = z.object({
  type: z.literal("joined"),
  seat: z.enum(["red", "yellow", "spectator"]),
  code: z.string(),
  you: IdentitySchema,
})

const StateSchema = z.object({
  type: z.literal("state"),
  board: BoardSchema,
  turn: z.enum(["red", "yellow"]),
  status: z.enum(["waiting", "playing", "won", "draw"]),
  winner: z.enum(["red", "yellow"]).optional(),
  winningCells: z.array(z.tuple([z.number(), z.number()])).optional(),
  players: z.record(z.string(), PlayerInfoSchema),
})

const MatchFoundSchema = z.object({
  type: z.literal("match_found"),
  code: z.string(),
})

const RematchOfferedSchema = z.object({
  type: z.literal("rematch_offered"),
  by: z.enum(["red", "yellow"]),
})

const SimpleSchema = z.object({
  type: z.enum(["opponent_disconnected", "opponent_reconnected", "rematch_started", "pong"]),
})

const ErrorSchema = z.object({
  type: z.literal("error"),
  code: z.string(),
  message: z.string(),
})

export const ServerMessageSchema = z.discriminatedUnion("type", [
  JoinedSchema,
  StateSchema,
  MatchFoundSchema,
  RematchOfferedSchema,
  SimpleSchema,
  ErrorSchema,
])
export type ServerMessage = z.infer<typeof ServerMessageSchema>

// ---- Client -> server messages ----------------------------------------------

export type ClientMessage =
  | { type: "create"; name: string }
  | { type: "join"; code: string; name: string }
  | { type: "find_match"; name: string }
  | { type: "move"; column: number }
  | { type: "rematch" }
  | { type: "leave" }
  | { type: "ping" }
