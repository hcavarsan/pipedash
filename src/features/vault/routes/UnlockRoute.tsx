import { Box, Center, Paper, Stack, Title } from '@mantine/core'
import type { ReactNode } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { AppLogo } from '../../../components/atoms/AppLogo'
import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { fromOf } from '../../../components/layout/mobileChrome'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useVaultStatus } from '../../../queries/useVaultQueries'
import { wsClient } from '../../../services/websocket'
import { isRecord } from '../../../utils/isRecord'
import { UnlockForm } from '../components/UnlockForm'

import classes from './UnlockRoute.module.css'

function returnToOf(state: unknown): string | null {
  return isRecord(state) && typeof state.returnTo === 'string' ? state.returnTo : null
}

export function UnlockRoute(): ReactNode {
  const navigate = useNavigate()
  const location = useLocation()
  const [searchParams] = useSearchParams()
  const requiresPassword = useVaultStatus().data?.requires_password ?? true
  const mobileShell = useMobileShell()

  const handleSuccess = (): void => {
    wsClient.manualReconnect()

    const returnTo = returnToOf(location.state) || searchParams.get('return') || '/pipelines'
    const from = fromOf(location.state)

    void navigate(returnTo, { replace: true, state: from ? { from } : undefined })
  }

  if (mobileShell) {
    return (
      <div className={classes.screen}>
        <DocumentTitle title="Unlock" />
        <div className={classes.hero}>
          <AppLogo size={76} alt="Pipedash" />
          <h1 className={classes.title}>{requiresPassword ? 'Unlock vault' : 'Unlock Pipedash'}</h1>
          <p className={classes.subtitle}>
            {requiresPassword ? 'Enter your password to continue.' : 'Pipedash is locked.'}
          </p>
        </div>
        <div className={classes.form}>
          <UnlockForm onSuccess={handleSuccess} />
        </div>
      </div>
    )
  }

  return (
    <Box component="main" h="100vh" className="safe-area">
      <DocumentTitle title="Unlock" />
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={{ base: '100%', xs: 420 }} withBorder>
          <Stack gap="xl">
            <Stack gap="md" align="center">
              <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
              <Title order={1} size="h2" ta="center">
                {requiresPassword ? 'Unlock vault' : 'Unlock Pipedash'}
              </Title>
            </Stack>

            <UnlockForm onSuccess={handleSuccess} />
          </Stack>
        </Paper>
      </Center>
    </Box>
  )
}
