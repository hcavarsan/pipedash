import { Center, Paper, Stack, Title } from '@mantine/core'
import { useCallback } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'

import { AppLogo } from '../components/atoms/AppLogo'
import { UnlockForm } from '../components/vault/UnlockForm'
import { wsClient } from '../services'
import { useAuthStore } from '../stores/authStore'

export function UnlockRoute() {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const consumeReturnPath = useAuthStore((s) => s.consumeReturnPath)

  const handleSuccess = useCallback(() => {
    // Reconnect WebSocket after unlock
    wsClient.manualReconnect()

    // Get return path from store or query param
    const storePath = consumeReturnPath()
    const queryPath = searchParams.get('return')
    const returnTo = storePath || queryPath || '/pipelines'

    navigate(returnTo, { replace: true })
  }, [consumeReturnPath, navigate, searchParams])

  return (
    <Center h="100vh" p={{ base: 'md', sm: 'xl' }}>
      <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={{ base: '100%', xs: 420 }} withBorder>
        <Stack gap="xl">
          <Stack gap="md" align="center">
            <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
            <Title order={2} ta="center">
              Unlock vault
            </Title>
          </Stack>

          <UnlockForm onSuccess={handleSuccess} />
        </Stack>
      </Paper>
    </Center>
  )
}
