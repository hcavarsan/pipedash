import { Button, Text } from '@mantine/core'
import { IconRefresh, IconSettings } from '@tabler/icons-react'
import { Link } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useRefreshProvider } from '../../queries/useRefreshQueries'
import type { FailureKind } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { describeRefreshResult } from '../../utils/refreshSummary'

const SETTINGS_KINDS: FailureKind[] = ['auth', 'permission', 'config', 'tls']

interface ProviderRecoveryActionsProps {
  providerId: number
  kind: FailureKind | null
  onNavigate?: () => void
}

export function ProviderRecoveryActions({ providerId, kind, onNavigate }: ProviderRecoveryActionsProps) {
  const refresh = useRefreshProvider()
  const buttonSize = useMobileShell() ? 'sm' : 'compact-xs'

  return (
    <>
      <Button
        size={buttonSize}
        variant="default"
        leftSection={<IconRefresh size={14} aria-hidden />}
        loading={refresh.isPending}
        onClick={() =>
          refresh.mutate(providerId, { onError: (error) => displayErrorNotification(error, "Couldn't refresh") })
        }
      >
        Retry now
      </Button>
      {kind && SETTINGS_KINDS.includes(kind) && (
        <Button
          component={Link}
          to="/settings/providers"
          size={buttonSize}
          variant="default"
          leftSection={<IconSettings size={14} aria-hidden />}
          onClick={onNavigate}
        >
          Open provider settings
        </Button>
      )}
      {refresh.data && (
        <Text size="xs" c="dimmed">
          {describeRefreshResult(refresh.data)}
        </Text>
      )}
    </>
  )
}
