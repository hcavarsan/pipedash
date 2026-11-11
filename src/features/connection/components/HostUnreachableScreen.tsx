import { Button, Stack, ThemeIcon } from '@mantine/core'
import { IconWifiOff } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { StatusScreen } from '../../../components/ErrorBoundary/StatusScreen'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { logger } from '../../../lib/logger'
import { connectionService, getConnection } from '../../../services/connection'
import { confirmDisconnect } from '../confirmDisconnect'
import { useDisconnectServer } from '../queries/useConnectionQueries'
import { EntryScreen } from './EntryScreen'

interface HostUnreachableScreenProps {
  onRetry: () => Promise<unknown>
}

export function HostUnreachableScreen({ onRetry }: HostUnreachableScreenProps): ReactNode {
  const mobileShell = useMobileShell()
  const disconnect = useDisconnectServer()
  const [retrying, setRetrying] = useState(false)
  const host = getConnection().host ?? 'the server'
  const title = `Couldn't connect to ${host}`
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
            <ThemeIcon variant="transparent" color="gray" size={48}>
              <IconWifiOff size={24} aria-hidden />
            </ThemeIcon>
          }
          title={title}
          headingOrder={1}
          message={message}
          actions={actions}
          minHeight="100dvh"
        />
      </>
    )
  }

  return (
    <EntryScreen title={title} message={message} wrapTitle documentTitle={title}>
      <Stack gap="sm">{actions}</Stack>
    </EntryScreen>
  )
}
