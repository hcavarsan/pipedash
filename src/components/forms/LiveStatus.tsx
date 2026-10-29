import { VisuallyHidden } from '@mantine/core'
import { type ReactNode, useEffect, useRef, useState } from 'react'

export function LiveStatus({ message }: { message: string }): ReactNode {
  return (
    <VisuallyHidden role="status" aria-live="polite" aria-atomic>
      {message}
    </VisuallyHidden>
  )
}

export function useErrorAnnouncement(errors: Record<string, string | undefined>): string {
  const previous = useRef<Record<string, string | undefined>>({})
  const [message, setMessage] = useState('')

  useEffect(() => {
    const fresh = Object.entries(errors)
      .filter(([key, text]) => text && previous.current[key] !== text)
      .map(([, text]) => text)

    previous.current = errors
    setMessage(fresh.join('. '))
  }, [errors])

  return message
}
