import { useEffect, useRef } from 'react'

import { Alert, Stack, Text } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'

export interface FormErrorDisplayProps {
  errors?: Record<string, string>
  globalError?: string | null
  onDismiss?: () => void
  compact?: boolean
}

export function FormErrorDisplay({
  errors,
  globalError,
  onDismiss,
  compact = false,
}: FormErrorDisplayProps) {
  const errorRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if ((globalError || (errors && Object.keys(errors).length > 0)) && errorRef.current) {
      errorRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }, [globalError, errors])

  const hasErrors = globalError || (errors && Object.keys(errors).length > 0)

  if (!hasErrors) {
    return null
  }

  const gap = compact ? 'xs' : 'sm'

  return (
    <Stack gap={gap} ref={errorRef}>
      {globalError && (
        <Alert
          icon={<IconAlertCircle size={16} />}
          title="Error"
          color="red"
          withCloseButton={!!onDismiss}
          onClose={onDismiss}
        >
          {globalError}
        </Alert>
      )}

      {errors && Object.keys(errors).length > 0 && (
        <Alert icon={<IconAlertCircle size={16} />} title="Validation Errors" color="red">
          <Stack gap={compact ? 4 : 8}>
            {Object.entries(errors).map(([field, message]) => (
              <Text key={field} size="sm">
                <strong>{field}:</strong> {message}
              </Text>
            ))}
          </Stack>
        </Alert>
      )}
    </Stack>
  )
}
