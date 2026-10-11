import { SIZE, type Grid } from "./types"

// Pure client-side helpers for instant feedback (conflict hints, empty grid).
// The Rust server (apps/sudoku-service/src/game/mod.rs) is the source of truth
// for correctness and completion; these never talk to the server.

export function emptyGrid(): Grid {
  return Array.from({ length: SIZE }, () => Array.from({ length: SIZE }, () => 0))
}

/** Number of non-empty cells. */
export function countFilled(grid: Grid): number {
  let n = 0
  for (const row of grid) for (const v of row) if (v !== 0) n++
  return n
}

/**
 * Cells that clash with another cell of the same value in their row, column, or
 * 3x3 box. Returned as a set of "r,c" keys for O(1) lookup while rendering. This
 * is a UX aid only — the server ignores conflicts and scores purely on
 * correctness against the hidden solution.
 */
export function conflicts(grid: Grid): Set<string> {
  const bad = new Set<string>()
  const mark = (cells: [number, number][]) => {
    const seen = new Map<number, [number, number]>()
    for (const [r, c] of cells) {
      const v = grid[r]?.[c] ?? 0
      if (v === 0) continue
      const prev = seen.get(v)
      if (prev) {
        bad.add(`${r},${c}`)
        bad.add(`${prev[0]},${prev[1]}`)
      } else {
        seen.set(v, [r, c])
      }
    }
  }

  for (let r = 0; r < SIZE; r++) {
    mark(Array.from({ length: SIZE }, (_, c) => [r, c] as [number, number]))
  }
  for (let c = 0; c < SIZE; c++) {
    mark(Array.from({ length: SIZE }, (_, r) => [r, c] as [number, number]))
  }
  for (let br = 0; br < SIZE; br += 3) {
    for (let bc = 0; bc < SIZE; bc += 3) {
      const cells: [number, number][] = []
      for (let r = br; r < br + 3; r++) for (let c = bc; c < bc + 3; c++) cells.push([r, c])
      mark(cells)
    }
  }
  return bad
}
