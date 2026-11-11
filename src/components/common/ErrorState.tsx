import { Alert, Button, Group, Stack, Text } from '@mantine/core'
import { IconAlertCircle, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { errorMessage } from '../../utils/errorDisplay'

import { ContentUnavailable } from './ContentUnavailable'

interface ErrorStateProps {
  title: string
  error: unknown
  hint?: ReactNode
  actions?: ReactNode
  onRetry?: () => void
  retrying?: boolean
}

interface RetryButtonProps {
  onRetry: () => void
  retrying: boolean
  compact: boolean
}

function RetryButton({ onRetry, retrying, compact }: RetryButtonProps): ReactNode {
  return (
    <Button
      {...(compact ? { size: 'xs' } : {})}
      variant="default"
      leftSection={<IconRefresh size={compact ? 14 : 16} aria-hidden />}
      loading={retrying}
      onClick={onRetry}
    >
      Retry
    </Button>
  )
}

export function ErrorState({ title, error, hint, actions, onRetry, retrying = false }: ErrorStateProps): ReactNode {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <ContentUnavailable
        role="alert"
        tone="error"
        icon={<IconAlertCircle size={24} aria-hidden />}
        title={title}
        description={errorMessage(error, 'Try again.')}
        hint={hint}
        actions={
          (onRetry || actions) && (
            <>
              {onRetry && <RetryButton onRetry={onRetry} retrying={retrying} compact={false} />}
              {actions}
            </>
          )
        }
      />
    )
  }

  return (
    <Alert color="red" variant="light" icon={<IconAlertCircle size={16} aria-hidden />} title={title}>
      <Stack gap="sm" align="flex-start">
        <Text size="sm">{errorMessage(error, 'Try again.')}</Text>
        {hint && (
          <Text size="sm" c="dimmed">
            {hint}
          </Text>
        )}
        {(onRetry || actions) && (
          <Group gap="xs">
            {onRetry && <RetryButton onRetry={onRetry} retrying={retrying} compact />}
            {actions}
          </Group>
        )}
      </Stack>
    </Alert>
  )
}
