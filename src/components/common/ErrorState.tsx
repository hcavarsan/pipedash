import { Alert, Button, Group, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { formatErrorMessage } from '../../types/errors'

import { ContentUnavailable } from './ContentUnavailable'

interface ErrorStateProps {
  title: string
  error: unknown
  hint?: ReactNode
  actions?: ReactNode
  onRetry?: () => void
  retrying?: boolean
}

export function ErrorState({ title, error, hint, actions, onRetry, retrying = false }: ErrorStateProps): ReactNode {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <ContentUnavailable
        role="alert"
        tone="error"
        icon={<IconAlertCircle />}
        title={title}
        description={formatErrorMessage(error)}
        hint={hint}
        actions={
          (onRetry || actions) && (
            <>
              {onRetry && (
                <Button variant="default" leftSection={<IconRefresh size={18} />} loading={retrying} onClick={onRetry}>
                  Retry
                </Button>
              )}
              {actions}
            </>
          )
        }
      />
    )
  }

  return (
    <Alert color="red" variant="light" icon={<IconAlertCircle size={16} />} title={title}>
      <Stack gap="sm" align="flex-start">
        <Text size="sm">{formatErrorMessage(error)}</Text>
        {hint && (
          <Text size="sm" c="dimmed">
            {hint}
          </Text>
        )}
        {(onRetry || actions) && (
          <Group gap="xs">
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
            {actions}
          </Group>
        )}
      </Stack>
    </Alert>
  )
}
