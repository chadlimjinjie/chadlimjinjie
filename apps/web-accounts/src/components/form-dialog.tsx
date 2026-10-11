import { useId, useState, type ComponentProps, type ReactNode } from 'react'
import { cn } from 'cn'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'

type FormProps = {
  submit?: string
  destructive?: boolean
  /** Receives the form's fields; a thrown error is shown under them. */
  onSubmit: (data: FormData) => Promise<unknown>
  onDone?: () => void
  className?: string
  children?: ReactNode
}

export function Form({ submit = 'Save', destructive, onSubmit, onDone, className, children }: FormProps) {
  const [state, setState] = useState<{ busy?: boolean; error?: string; saved?: boolean }>({})
  return (
    <form
      className={cn('grid gap-4', className)}
      onSubmit={async (e) => {
        e.preventDefault()
        const data = new FormData(e.currentTarget)
        setState({ busy: true })
        try {
          await onSubmit(data)
          setState({ saved: true })
          onDone?.()
        } catch (err) {
          setState({ error: err instanceof Error ? err.message : String(err) })
        }
      }}
    >
      {children}
      {state.error && (
        <p role="alert" className="text-sm text-destructive">
          {state.error}
        </p>
      )}
      <div className="flex items-center justify-end gap-3">
        {state.saved && (
          <span role="status" className="text-sm text-muted-foreground">
            Saved
          </span>
        )}
        <Button type="submit" variant={destructive ? 'destructive' : 'default'} disabled={state.busy}>
          {submit}
        </Button>
      </div>
    </form>
  )
}

/** A button that opens a form in a dialog and closes it once the submit succeeds. With no fields it is a confirmation. */
export function FormDialog({
  trigger,
  variant = 'default',
  title = trigger,
  description,
  ...form
}: Omit<FormProps, 'onDone' | 'className'> & {
  trigger: string
  variant?: ComponentProps<typeof Button>['variant']
  title?: string
  description?: string
}) {
  const [open, setOpen] = useState(false)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant={form.destructive ? 'destructive' : variant} size="sm" />}>
        {trigger}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
        </DialogHeader>
        <Form {...form} onDone={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  )
}

export function Field({ label, hint, ...props }: ComponentProps<typeof Input> & { label: string; hint?: string }) {
  const id = useId()
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input id={id} aria-describedby={hint ? `${id}-hint` : undefined} {...props} />
      {hint && (
        <p id={`${id}-hint`} className="text-xs text-muted-foreground">
          {hint}
        </p>
      )}
    </div>
  )
}

/** A labelled switch; in a form it submits "on" under its name when checked. */
export function Toggle({ label, ...props }: ComponentProps<typeof Switch> & { label: string }) {
  return (
    <Label className="flex items-center gap-2">
      <Switch {...props} />
      {label}
    </Label>
  )
}
