import { Alert, Stack, Text } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { useEffect, useRef } from 'react'

export interface FormErrorDisplayProps {
  errors?: Record<string, string>
  globalError?: string | null
  onDismiss?: () => void
  compact?: boolean
}

export function FormErrorDisplay({ errors, globalError, onDismiss, compact = false }: FormErrorDisplayProps) {
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
          title="Couldn't save"
          color="red"
          variant="light"
          withCloseButton={!!onDismiss}
          closeButtonLabel="Dismiss error"
          onClose={onDismiss}
        >
          {globalError}
        </Alert>
      )}

      {errors && Object.keys(errors).length > 0 && (
        <Alert icon={<IconAlertCircle size={16} />} title="Validation errors" color="red" variant="light">
          <Stack gap={gap}>
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
