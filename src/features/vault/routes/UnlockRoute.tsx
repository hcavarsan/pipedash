import { Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { fromOf } from '../../../components/layout/mobileChrome'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useVaultStatus } from '../../../queries/useVaultQueries'
import { wsClient } from '../../../services/websocket'
import { isRecord } from '../../../utils/isRecord'
import { EntryScreen } from '../../connection/components/EntryScreen'
import { Onboarding } from '../../setup/components/SetupWizard/Onboarding'
import { UnlockForm } from '../components/UnlockForm'

function appPath(value: unknown): string | null {
  return typeof value === 'string' && /^\/(?![/\\])/.test(value) ? value : null
}

export function UnlockRoute(): ReactNode {
  const navigate = useNavigate()
  const location = useLocation()
  const [searchParams] = useSearchParams()
  const requiresPassword = useVaultStatus().data?.requires_password ?? true
  const mobileShell = useMobileShell()

  const handleSuccess = (): void => {
    wsClient.manualReconnect()

    const stateReturnTo = isRecord(location.state) ? location.state.returnTo : null
    const returnTo = appPath(stateReturnTo) ?? appPath(searchParams.get('return')) ?? '/'
    const from = fromOf(location.state)

    void navigate(returnTo, { replace: true, state: from ? { from } : undefined })
  }

  const title = requiresPassword ? 'Unlock vault' : 'Unlock Pipedash'

  if (mobileShell) {
    return (
      <Onboarding title={title}>
        <DocumentTitle title="Unlock" />
        <Text size="sm" c="dimmed" ta="center" mb="md">
          {requiresPassword ? 'Enter your password to continue.' : 'Pipedash is locked.'}
        </Text>
        <UnlockForm onSuccess={handleSuccess} />
      </Onboarding>
    )
  }

  return (
    <EntryScreen title={title} documentTitle="Unlock">
      <UnlockForm onSuccess={handleSuccess} />
    </EntryScreen>
  )
}
