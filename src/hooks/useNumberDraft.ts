import { useRef, useState } from 'react'

import { errorMessage } from '../utils/errorDisplay'

interface NumberDraftOptions {
  saved: number
  min: number
  save: (value: number) => Promise<void>
}

interface NumberDraft {
  value: string | number
  error: string | null
  pending: boolean
  onChange: (value: string | number) => void
  commit: () => void
}

interface Draft {
  value: string | number
  dirty: boolean
}

export function useNumberDraft({ saved, min, save }: NumberDraftOptions): NumberDraft {
  const [draft, setDraft] = useState<Draft | null>(null)
  const [seenSaved, setSeenSaved] = useState(saved)
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const inFlight = useRef(false)
  const latest = useRef<string | number | undefined>(undefined)

  if (seenSaved !== saved) {
    setSeenSaved(saved)

    if (draft && !draft.dirty) {
      setDraft(null)
    }
  }

  const value = draft?.value ?? saved
  latest.current = draft?.value

  const onChange = (next: string | number): void => {
    setDraft({ value: next, dirty: true })
    setError(null)
  }

  const run = async (target: number, submitted: string | number): Promise<void> => {
    inFlight.current = true
    setPending(true)

    try {
      await save(target)
      setDraft((current) => (current?.value === submitted ? { value: submitted, dirty: false } : current))
    } catch (err) {
      if (latest.current === undefined || latest.current === submitted) {
        setDraft(null)
        setError(errorMessage(err, "Couldn't save the value"))
      }
    } finally {
      inFlight.current = false
      setPending(false)
    }
  }

  const commit = (): void => {
    if (inFlight.current) {
      return
    }

    if (typeof value === 'string' && value.trim() === '') {
      setError('Enter a number')

      return
    }

    const target = Number(value)

    if (!Number.isInteger(target) || target < min) {
      setError(`Must be at least ${min}`)

      return
    }

    if (target === saved) {
      setDraft(null)

      return
    }

    setError(null)
    void run(target, value)
  }

  return { value, error, pending, onChange, commit }
}
