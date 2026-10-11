import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { getPlayerName, setPlayerName } from "@/lib/player"

export function ConnectFourLanding() {
  const navigate = useNavigate()
  const [name, setName] = useState(getPlayerName)
  const [code, setCode] = useState("")

  const go = (code: string) => {
    setPlayerName(name.trim() || "Player")
    navigate({ to: "/connect-four/$code", params: { code } })
  }

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-4">
      <div className="w-full max-w-md space-y-6">
        <div className="text-center">
          <h1 className="text-2xl font-semibold tracking-tight">Connect Four</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Play a friend in real time — no account needed.
          </p>
        </div>

        <div className="space-y-2">
          <label htmlFor="cf-name" className="text-sm font-medium">
            Display name
          </label>
          <Input
            id="cf-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Your name"
            maxLength={24}
          />
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <Button onClick={() => go("new")}>Create game</Button>
          <Button variant="secondary" onClick={() => go("find")}>
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
          <label htmlFor="cf-code" className="text-sm font-medium">
            Join by code
          </label>
          <div className="flex gap-2">
            <Input
              id="cf-code"
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
