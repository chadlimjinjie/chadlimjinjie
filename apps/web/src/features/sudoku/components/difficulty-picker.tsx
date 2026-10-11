import { Button } from "@/components/ui/button"
import type { Difficulty } from "../types"

interface DifficultyPickerProps {
  value: Difficulty
  onChange: (value: Difficulty) => void
}

const DIFFICULTIES: Difficulty[] = ["easy", "medium", "hard"]

/** Easy / Medium / Hard selector for the create/find flows. */
export function DifficultyPicker({ value, onChange }: DifficultyPickerProps) {
  return (
    <div role="group" aria-label="Difficulty" className="flex gap-2">
      {DIFFICULTIES.map((d) => (
        <Button
          key={d}
          variant={d === value ? "default" : "outline"}
          aria-pressed={d === value}
          onClick={() => onChange(d)}
          className="flex-1 capitalize"
        >
          {d}
        </Button>
      ))}
    </div>
  )
}
