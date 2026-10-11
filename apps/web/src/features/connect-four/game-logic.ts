import { ROWS, COLS, type Board, type Cell } from "./types"

// Pure Connect Four rules, mirroring the authoritative Go implementation
// (apps/connect-four-service/internal/game/board.go). Used client-side only for
// instant feedback (disabling full columns, optimistic checks); the server's
// snapshot is always the source of truth.

export function emptyBoard(): Board {
  return Array.from({ length: ROWS }, () => Array.from({ length: COLS }, () => "" as Cell))
}

/** Columns that still have room for a disc. */
export function legalColumns(board: Board): number[] {
  const cols: number[] = []
  for (let c = 0; c < COLS; c++) {
    if (board[0]?.[c] === "") cols.push(c)
  }
  return cols
}

export function isColumnFull(board: Board, col: number): boolean {
  return board[0]?.[col] !== ""
}

/** Drops a disc into col, returning a new board and the landing row, or null. */
export function applyMove(
  board: Board,
  col: number,
  color: Exclude<Cell, "">,
): { board: Board; row: number } | null {
  if (col < 0 || col >= COLS) return null
  for (let row = ROWS - 1; row >= 0; row--) {
    if (board[row][col] === "") {
      const next = board.map((r) => r.slice())
      next[row][col] = color
      return { board: next, row }
    }
  }
  return null
}

const DIRECTIONS: ReadonlyArray<[number, number]> = [
  [0, 1],
  [1, 0],
  [1, 1],
  [1, -1],
]

function inBounds(r: number, c: number): boolean {
  return r >= 0 && r < ROWS && c >= 0 && c < COLS
}

/** Winning cells through (row,col), or null if there is no four-in-a-row. */
export function detectWin(board: Board, row: number, col: number): [number, number][] | null {
  const color = board[row]?.[col]
  if (!color) return null
  for (const [dr, dc] of DIRECTIONS) {
    const cells: [number, number][] = [[row, col]]
    for (let k = 1; inBounds(row + dr * k, col + dc * k); k++) {
      if (board[row + dr * k][col + dc * k] !== color) break
      cells.push([row + dr * k, col + dc * k])
    }
    for (let k = 1; inBounds(row - dr * k, col - dc * k); k++) {
      if (board[row - dr * k][col - dc * k] !== color) break
      cells.push([row - dr * k, col - dc * k])
    }
    if (cells.length >= 4) return cells
  }
  return null
}

export function isFull(board: Board): boolean {
  return legalColumns(board).length === 0
}
