import { useCallback } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'

import { Box, Center, Image, Paper, Stack, Title } from '@mantine/core'

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
    <Center
      h="100vh"
      bg="var(--mantine-color-body)"
      p={{ base: 'md', sm: 'xl' }}
    >
      <Paper
        shadow="lg"
        radius="lg"
        p={{ base: 'lg', sm: 'xl' }}
        w="100%"
        maw={{ base: '100%', xs: 420 }}
        withBorder
      >
        <Stack gap="xl">
          <Stack gap="md" align="center">
            <Box
              p="md"
              style={{
                borderRadius: 'var(--mantine-radius-xl)',
                background: 'var(--mantine-color-dark-6)',
              }}
            >
              <Image
                src="/app-icon.png"
                w={{ base: 48, sm: 56 }}
                h={{ base: 48, sm: 56 }}
              />
            </Box>
            <Title order={2} ta="center">
              Unlock Vault
            </Title>
          </Stack>

          <UnlockForm onSuccess={handleSuccess} />
        </Stack>
      </Paper>
    </Center>
  )
}
