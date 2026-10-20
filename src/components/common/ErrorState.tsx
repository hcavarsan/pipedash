import { Alert, Button, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { formatErrorMessage } from '../../types/errors'

interface ErrorStateProps {
  title: string
  error: unknown
  hint?: ReactNode
  onRetry?: () => void
  retrying?: boolean
}

export function ErrorState({ title, error, hint, onRetry, retrying = false }: ErrorStateProps) {
  return (
    <Alert color="red" variant="light" icon={<IconAlertCircle size={16} />} title={title}>
      <Stack gap="sm" align="flex-start">
        <Text size="sm">{formatErrorMessage(error)}</Text>
        {hint && (
          <Text size="sm" c="dimmed">
            {hint}
          </Text>
        )}
        {onRetry && (
          <Button
            size="xs"
            variant="default"
            leftSection={<IconRefresh size={14} />}
            loading={retrying}
            onClick={onRetry}
          >
            Retry
          </Button>
        )}
      </Stack>
    </Alert>
  )
}
