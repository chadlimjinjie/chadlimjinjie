import { createFileRoute } from '@tanstack/react-router'
import { SudokuRoom } from '@/features/sudoku/components/sudoku-room'
import type { SudokuMode } from '@/features/sudoku/hooks/use-sudoku'
import type { Difficulty } from '@/features/sudoku/types'

export const Route = createFileRoute('/sudoku/$code')({
  // Difficulty rides on `?d=`; the server is authoritative, so anything else means medium.
  validateSearch: (search): { d?: Difficulty } =>
    search.d === 'easy' || search.d === 'medium' || search.d === 'hard' ? { d: search.d } : {},
  component: Room,
})

function Room() {
  const { code } = Route.useParams()
  const { d: difficulty = 'medium' } = Route.useSearch()
  // `new` and `find` are reserved pseudo-codes for the create / random-match flows.
  const lower = code.toLowerCase()
  const mode: SudokuMode =
    lower === 'new'
      ? { kind: 'create', difficulty }
      : lower === 'find'
        ? { kind: 'find', difficulty }
        : { kind: 'join', code: code.toUpperCase() }
  return <SudokuRoom mode={mode} />
}
