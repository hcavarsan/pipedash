import { Box, Button, Center, Paper, Stack } from '@mantine/core'
import { IconPlugConnectedX } from '@tabler/icons-react'
import { type ReactNode, useEffect, useState } from 'react'

import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { StatusScreen } from '../../../components/ErrorBoundary/StatusScreen'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { logger } from '../../../lib/logger'
import { getConnection, REMOTE_ACCESS_REVOKED_EVENT } from '../../../services/connection'
import { listenEvent } from '../../../services/events'
import { useDisconnectServer } from '../queries/useConnectionQueries'
import { ConnectionScreenHeading } from './ConnectionScreenHeading'

const TITLE = 'This device was removed'

export function ServerAccessGate({ children }: { children: ReactNode }): ReactNode {
  const [removed, setRemoved] = useState(() => getConnection().status === 'removed')
  const mobileShell = useMobileShell()
  const disconnect = useDisconnectServer()

  useEffect(() => {
    let active = true
    let stop: (() => void) | undefined

    listenEvent(REMOTE_ACCESS_REVOKED_EVENT, () => setRemoved(true))
      .then((unlisten) => {
        if (active) {
          stop = unlisten
        } else {
          unlisten()
        }
      })
      .catch((error: unknown) => logger.warn('ServerAccessGate', "Couldn't listen for revoked access", error))

    return () => {
      active = false
      stop?.()
    }
  }, [])

  if (!removed) {
    return children
  }

  const message = `${getConnection().host ?? 'The server'} no longer accepts this device.`
  const action = (
    <Button
      fullWidth
      size={mobileShell ? 'lg' : 'md'}
      loading={disconnect.isPending}
      onClick={() => disconnect.mutate()}
    >
      Set up again
    </Button>
  )

  if (mobileShell) {
    return (
      <>
        <DocumentTitle title={TITLE} />
        <StatusScreen
          icon={
            <Box c="dimmed" lh={0}>
              <IconPlugConnectedX size={48} aria-hidden />
            </Box>
          }
          title={TITLE}
          message={message}
          actions={action}
          minHeight="100dvh"
        />
      </>
    )
  }

  return (
    <Box component="main" h="100vh" className="safe-area">
      <DocumentTitle title={TITLE} />
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={480} withBorder>
          <Stack gap="xl">
            <ConnectionScreenHeading title={TITLE} message={message} />

            {action}
          </Stack>
        </Paper>
      </Center>
    </Box>
  )
}
