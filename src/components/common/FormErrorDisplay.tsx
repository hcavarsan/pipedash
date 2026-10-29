import { Alert } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { type ReactNode, useEffect, useRef } from 'react'

import { scrollBehavior } from '../../lib/motion'

interface FormErrorDisplayProps {
  title: string
  globalError?: string | null | undefined
  onDismiss?: (() => void) | undefined
}

export function FormErrorDisplay({ title, globalError, onDismiss }: FormErrorDisplayProps): ReactNode {
  const errorRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (globalError) {
      errorRef.current?.scrollIntoView({ behavior: scrollBehavior(), block: 'nearest' })
    }
  }, [globalError])

  if (!globalError) {
    return null
  }

  return (
    <Alert
      ref={errorRef}
      icon={<IconAlertCircle size={16} aria-hidden />}
      title={title}
      color="red"
      variant="light"
      withCloseButton={!!onDismiss}
      closeButtonLabel="Dismiss error"
      {...(onDismiss ? { onClose: onDismiss } : {})}
    >
      {globalError}
    </Alert>
  )
}
