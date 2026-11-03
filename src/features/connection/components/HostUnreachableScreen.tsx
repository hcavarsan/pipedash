import { Box, Button, Center, Paper, Stack, Text, Title } from '@mantine/core'
import { IconWifiOff } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { AppLogo } from '../../../components/atoms/AppLogo'
import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { StatusScreen } from '../../../components/ErrorBoundary/StatusScreen'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { logger } from '../../../lib/logger'
import { connectionService, getConnection } from '../../../services/connection'
import { confirmDisconnect } from '../confirmDisconnect'
import { useDisconnectServer } from '../queries/useConnectionQueries'

interface HostUnreachableScreenProps {
  onRetry: () => Promise<unknown>
}

export function HostUnreachableScreen({ onRetry }: HostUnreachableScreenProps): ReactNode {
  const mobileShell = useMobileShell()
  const disconnect = useDisconnectServer()
  const [retrying, setRetrying] = useState(false)
  const host = getConnection().host ?? 'the server'
  const title = `Can't reach ${host}`
  const message = `Check that ${host} is running and that this device can reach it.`
  const size = mobileShell ? 'lg' : 'md'

  const retry = async (): Promise<void> => {
    setRetrying(true)

    try {
      await connectionService.reconnect()
      await onRetry()
    } catch (error) {
      logger.warn('HostUnreachableScreen', "Couldn't reconnect", error)
    } finally {
      setRetrying(false)
    }
  }

  const handleDisconnect = async (): Promise<void> => {
    if (await confirmDisconnect(host)) {
      disconnect.mutate()
    }
  }

  const actions = (
    <>
      <Button fullWidth size={size} loading={retrying} disabled={disconnect.isPending} onClick={() => void retry()}>
        Try again
      </Button>
      <Button
        fullWidth
        size={size}
        variant="subtle"
        color="gray"
        loading={disconnect.isPending}
        disabled={retrying}
        onClick={() => void handleDisconnect()}
      >
        Disconnect
      </Button>
    </>
  )

  if (mobileShell) {
    return (
      <>
        <DocumentTitle title={title} />
        <StatusScreen
          icon={
            <Box c="dimmed" lh={0}>
              <IconWifiOff size={48} aria-hidden />
            </Box>
          }
          title={title}
          message={message}
          actions={actions}
          minHeight="100dvh"
        />
      </>
    )
  }

  return (
    <Box component="main" h="100vh" className="safe-area">
      <DocumentTitle title={title} />
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={480} withBorder>
          <Stack gap="xl">
            <Stack gap="md" align="center">
              <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
              <Title order={1} size="h2" ta="center" style={{ overflowWrap: 'anywhere' }}>
                {title}
              </Title>
              <Text c="dimmed" ta="center" style={{ overflowWrap: 'anywhere' }}>
                {message}
              </Text>
            </Stack>

            <Stack gap="sm">{actions}</Stack>
          </Stack>
        </Paper>
      </Center>
    </Box>
  )
}
