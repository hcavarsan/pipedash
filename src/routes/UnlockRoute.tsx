import { Box, Center, Paper, Stack, Title } from '@mantine/core'
import { useCallback } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'

import { AppLogo } from '../components/atoms/AppLogo'
import { UnlockForm } from '../components/vault/UnlockForm'
import { useMobileShell } from '../hooks/useIsMobile'
import { useVaultStatus } from '../queries/useVaultQueries'
import { wsClient } from '../services'
import { useAuthStore } from '../stores/authStore'
import { useMobileNavStore } from '../stores/mobileNavStore'

import classes from './UnlockRoute.module.css'

export function UnlockRoute() {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const consumeReturnPath = useAuthStore((s) => s.consumeReturnPath)
  const requiresPassword = useVaultStatus().data?.requires_password ?? true
  const takeHeld = useMobileNavStore((s) => s.takeHeld)
  const mobileShell = useMobileShell()

  const handleSuccess = useCallback(() => {
    // Reconnect WebSocket after unlock
    wsClient.manualReconnect()

    // Get return path from store or query param
    const storePath = consumeReturnPath()
    const queryPath = searchParams.get('return')
    const returnTo = storePath || queryPath || '/pipelines'
    const from = takeHeld(returnTo)

    navigate(returnTo, { replace: true, state: from ? { from } : undefined })
  }, [consumeReturnPath, navigate, searchParams, takeHeld])

  if (mobileShell) {
    return (
      <div className={classes.screen}>
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
    <Box h="100vh" className="safe-area">
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={{ base: '100%', xs: 420 }} withBorder>
          <Stack gap="xl">
            <Stack gap="md" align="center">
              <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
              <Title order={2} ta="center">
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
