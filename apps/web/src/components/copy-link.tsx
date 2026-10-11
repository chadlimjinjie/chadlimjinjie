import { useState } from 'react'
import { Check, Copy } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

export function CopyLink({ url }: { url: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    // Clipboard can be denied; the link stays selectable in the input either way.
    try {
      await navigator.clipboard.writeText(url)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      setCopied(false)
    }
  }

  return (
    <div className="flex w-full max-w-sm items-center gap-2">
      <Input readOnly value={url} onFocus={(e) => e.currentTarget.select()} className="text-sm" />
      <Button type="button" variant="outline" size="icon" onClick={copy} aria-label="Copy link">
        {copied ? <Check /> : <Copy />}
      </Button>
      <span role="status" className="sr-only">
        {copied ? 'Link copied' : ''}
      </span>
    </div>
  )
}
