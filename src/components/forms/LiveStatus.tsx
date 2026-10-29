import { VisuallyHidden } from '@mantine/core'
import { type ReactNode, useState } from 'react'

export function LiveStatus({ message }: { message: string }): ReactNode {
  return (
    <VisuallyHidden role="status" aria-live="polite" aria-atomic>
      {message}
    </VisuallyHidden>
  )
}

interface AnnouncementState {
  errors: Record<string, string | undefined>
  message: string
}

export function useErrorAnnouncement(errors: Record<string, string | undefined>): string {
  const [state, setState] = useState<AnnouncementState>({ errors: {}, message: '' })

  if (state.errors !== errors) {
    const fresh = Object.entries(errors)
      .filter(([key, text]) => text && state.errors[key] !== text)
      .map(([, text]) => text)

    setState({ errors, message: fresh.join('. ') })
  }

  return state.message
}
