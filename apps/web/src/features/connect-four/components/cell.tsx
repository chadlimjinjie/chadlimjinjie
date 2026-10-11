import { cva } from "class-variance-authority"

import { cn } from "@/lib/utils"
import type { Cell as CellValue } from "../types"

const discVariants = cva("aspect-square w-9 rounded-full transition-all sm:w-11", {
  variants: {
    color: {
      "": "bg-background/90 shadow-inner",
      red: "bg-red-500 shadow",
      yellow: "bg-yellow-400 shadow",
    },
    winning: {
      true: "scale-105 ring-4 ring-white",
      false: "",
    },
  },
  defaultVariants: { color: "", winning: false },
})

export function Cell({ value, winning }: { value: CellValue; winning: boolean }) {
  return (
    <div
      className={cn(
        discVariants({ color: value, winning }),
        value !== "" && "animate-in slide-in-from-top-2 duration-200",
      )}
    />
  )
}
