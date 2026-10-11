import { createFileRoute } from '@tanstack/react-router'
import { SudokuLanding } from '@/features/sudoku/components/sudoku-page'

export const Route = createFileRoute('/sudoku/')({
  component: SudokuLanding,
})
