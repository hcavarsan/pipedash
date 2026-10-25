import { Alert, Button, Group, Stack, Text, ThemeIcon } from '@mantine/core'
import { IconAlertCircle, IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { formatErrorMessage } from '../../types/errors'

interface ErrorStateProps {
  title: string
  error: unknown
  hint?: ReactNode
  actions?: ReactNode
  onRetry?: () => void
  retrying?: boolean
}

export function ErrorState({ title, error, hint, actions, onRetry, retrying = false }: ErrorStateProps) {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return (
      <Stack align="center" gap="xs" py={48} px="md" ta="center" role="alert">
        <ThemeIcon variant="transparent" color="red" size={48} aria-hidden>
          <IconAlertCircle size={36} />
        </ThemeIcon>
        <Text fw={600} size="lg">
          {title}
        </Text>
        <Text size="sm" c="dimmed">
          {formatErrorMessage(error)}
        </Text>
        {hint && (
          <Text size="sm" c="dimmed">
            {hint}
          </Text>
        )}
        {(onRetry || actions) && (
          <Group gap="sm" justify="center" mt="xs">
            {onRetry && (
              <Button variant="default" leftSection={<IconRefresh size={18} />} loading={retrying} onClick={onRetry}>
                Retry
              </Button>
            )}
            {actions}
          </Group>
        )}
      </Stack>
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
