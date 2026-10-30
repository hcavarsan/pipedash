import { Button, Text } from '@mantine/core'
import { IconLogin, IconRefresh, IconSettings } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import { useProviderSignIn } from '../../features/providers/queries/useProviderSignIn'
import { useMobileShell } from '../../hooks/useIsMobile'
import { useRefreshProvider } from '../../queries/useRefreshQueries'
import type { ProviderRecovery } from '../../types/domain'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { describeRefreshResult } from '../../utils/refreshSummary'

interface ProviderRecoveryActionsProps {
  providerId: number
  recovery: ProviderRecovery | null
  onNavigate?: () => void
}

export function ProviderRecoveryActions({ providerId, recovery, onNavigate }: ProviderRecoveryActionsProps): ReactNode {
  const refresh = useRefreshProvider()
  const buttonSize = useMobileShell() ? 'sm' : 'compact-xs'
  const signIn = useProviderSignIn()

  return (
    <>
      {recovery !== 'sign_in' && (
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
      )}
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
          variant="filled"
          leftSection={<IconLogin size={14} aria-hidden />}
          onClick={() => {
            onNavigate?.()
            signIn(providerId)
          }}
        >
          Sign in
        </Button>
      )}
      <Text size="xs" c="dimmed" role="status" aria-live="polite">
        {refresh.data ? describeRefreshResult(refresh.data) : null}
      </Text>
    </>
  )
}
