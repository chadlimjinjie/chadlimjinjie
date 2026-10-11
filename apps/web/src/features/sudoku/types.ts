import { z } from "zod"

/** Board dimension — must match the Rust server (game/mod.rs, SIZE = 9). */
export const SIZE = 9

/** Difficulty the creator picks; both players get the same puzzle. */
export type Difficulty = "easy" | "medium" | "hard"

/** A seated player. Sudoku has no colours, so seats are just p1/p2. */
export type Seat = "p1" | "p2"

/** Game lifecycle. */
export type Status = "waiting" | "playing" | "over"

/** From-the-winner's perspective, relative to the recipient. */
export type Winner = "you" | "opponent" | "draw"

/** A 9x9 grid of digits; 0 means empty. */
export const GridSchema = z.array(z.array(z.number().int().min(0).max(9)))
export type Grid = z.infer<typeof GridSchema>

/** A 9x9 boolean mask (which cells the opponent has filled). */
export const MaskSchema = z.array(z.array(z.boolean()))
export type Mask = z.infer<typeof MaskSchema>

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
  seat: z.enum(["p1", "p2"]),
  code: z.string(),
  you: IdentitySchema,
})

// The opponent is represented ONLY by `oppFilled` (a boolean mask) and
// `oppCorrect` (a count) — their digits are never sent. `board` is the
// recipient's own grid. This mirrors the server's per-recipient redaction.
const StateSchema = z.object({
  type: z.literal("state"),
  status: z.enum(["waiting", "playing", "over"]),
  difficulty: z.enum(["easy", "medium", "hard"]),
  givens: GridSchema,
  board: GridSchema,
  yourCorrect: z.number(),
  oppFilled: MaskSchema,
  oppCorrect: z.number(),
  deadline: z.number().nullable(),
  winner: z.enum(["you", "opponent", "draw"]).optional(),
  players: z.record(z.string(), PlayerInfoSchema),
})

const MatchFoundSchema = z.object({
  type: z.literal("match_found"),
  code: z.string(),
})

const GameOverSchema = z.object({
  type: z.literal("game_over"),
  winner: z.enum(["you", "opponent", "draw"]),
  yourCorrect: z.number(),
  oppCorrect: z.number(),
})

const RematchOfferedSchema = z.object({
  type: z.literal("rematch_offered"),
  by: z.enum(["p1", "p2"]),
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
  GameOverSchema,
  RematchOfferedSchema,
  SimpleSchema,
  ErrorSchema,
])
export type ServerMessage = z.infer<typeof ServerMessageSchema>

// ---- Client -> server messages ----------------------------------------------

export type ClientMessage =
  | { type: "create"; difficulty: Difficulty; name: string }
  | { type: "join"; code: string; name: string }
  | { type: "find_match"; name: string; difficulty?: Difficulty }
  | { type: "set_cell"; row: number; col: number; value: number }
  | { type: "clear_cell"; row: number; col: number }
  | { type: "rematch" }
  | { type: "leave" }
  | { type: "ping" }
