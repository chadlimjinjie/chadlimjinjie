import { Eraser } from "lucide-react"

import { Button } from "@/components/ui/button"

interface NumberPadProps {
  disabled: boolean
  onInput: (value: number) => void
  onErase: () => void
}

/** Touch-friendly 1–9 + erase input for the selected cell (mobile primary input). */
export function NumberPad({ disabled, onInput, onErase }: NumberPadProps) {
  return (
    <div className="grid w-full max-w-[min(90vw,26rem)] grid-cols-5 gap-1.5">
      {Array.from({ length: 9 }, (_, i) => i + 1).map((n) => (
        <Button
          key={n}
          type="button"
          variant="outline"
          size="lg"
          disabled={disabled}
          onClick={() => onInput(n)}
          className="text-lg font-semibold tabular-nums"
        >
          {n}
        </Button>
      ))}
      <Button
        type="button"
        variant="outline"
        size="lg"
        disabled={disabled}
        onClick={onErase}
        aria-label="Erase"
      >
        <Eraser className="size-5" />
      </Button>
    </div>
  )
}
