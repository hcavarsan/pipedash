import { Alert } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { useEffect, useRef } from 'react'

import { scrollBehavior } from '../../lib/motion'

interface FormErrorDisplayProps {
  globalError?: string | null
  onDismiss?: () => void
}

export function FormErrorDisplay({ globalError, onDismiss }: FormErrorDisplayProps) {
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
      icon={<IconAlertCircle size={16} />}
      title="Couldn't save"
      color="red"
      variant="light"
      withCloseButton={!!onDismiss}
      closeButtonLabel="Dismiss error"
      onClose={onDismiss}
    >
      {globalError}
    </Alert>
  )
}
