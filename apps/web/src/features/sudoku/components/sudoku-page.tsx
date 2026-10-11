import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { getPlayerName, setPlayerName } from "@/lib/player"
import type { Difficulty } from "../types"
import { DifficultyPicker } from "./difficulty-picker"

export function SudokuLanding() {
  const navigate = useNavigate()
  const [name, setName] = useState(getPlayerName)
  const [difficulty, setDifficulty] = useState<Difficulty>("medium")
  const [code, setCode] = useState("")

  const go = (code: string, d?: Difficulty) => {
    setPlayerName(name.trim() || "Player")
    navigate({ to: "/sudoku/$code", params: { code }, search: { d } })
  }

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-4 py-8">
      <div className="w-full max-w-md space-y-6">
        <div className="text-center">
          <h1 className="text-2xl font-semibold tracking-tight">Sudoku Duel</h1>
          <p className="text-muted-foreground mt-1 text-sm">
            Same puzzle, 5-minute race. Most correct cells wins — no account needed.
          </p>
        </div>

        <div className="space-y-2">
          <label htmlFor="sk-name" className="text-sm font-medium">
            Display name
          </label>
          <Input
            id="sk-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Your name"
            maxLength={24}
          />
        </div>

        <div className="space-y-2">
          <span className="text-sm font-medium">Difficulty</span>
          <DifficultyPicker value={difficulty} onChange={setDifficulty} />
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <Button onClick={() => go("new", difficulty)}>Create game</Button>
          <Button variant="secondary" onClick={() => go("find", difficulty)}>
            Find random match
          </Button>
        </div>

        <form
          onSubmit={(e) => {
            e.preventDefault()
            const c = code.trim().toUpperCase()
            if (c) go(c)
          }}
          className="space-y-2"
        >
          <label htmlFor="sk-code" className="text-sm font-medium">
            Join by code
          </label>
          <div className="flex gap-2">
            <Input
              id="sk-code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="ABC123"
              className="uppercase"
              maxLength={6}
            />
            <Button type="submit" variant="outline" disabled={!code.trim()}>
              Join
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}
