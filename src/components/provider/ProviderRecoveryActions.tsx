import { Button, Text } from '@mantine/core'
import { IconLogin, IconRefresh, IconSettings } from '@tabler/icons-react'
import { Link } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useRefreshProvider } from '../../queries/useRefreshQueries'
import { useProviderStore } from '../../stores/providerStore'
import type { ProviderRecovery } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { describeRefreshResult } from '../../utils/refreshSummary'

interface ProviderRecoveryActionsProps {
  providerId: number
  recovery: ProviderRecovery | null
  onNavigate?: () => void
}

export function ProviderRecoveryActions({ providerId, recovery, onNavigate }: ProviderRecoveryActionsProps) {
  const refresh = useRefreshProvider()
  const buttonSize = useMobileShell() ? 'sm' : 'compact-xs'
  const requestEditProvider = useProviderStore((state) => state.requestEditProvider)

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
      {recovery === 'settings' && (
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
      {recovery === 'sign_in' && (
        <Button
          size={buttonSize}
          variant="default"
          leftSection={<IconLogin size={14} aria-hidden />}
          onClick={() => {
            onNavigate?.()
            requestEditProvider(providerId)
          }}
        >
          Sign in again
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
