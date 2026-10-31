import { Alert, Box, Button, Loader, Stack, Text } from '@mantine/core'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { type ReactNode, useEffect, useRef } from 'react'

import {
  applyUpdateStatus,
  type UpdateConnection,
  useUpdateConnection,
  useUpdateStatus,
} from '../features/settings/queries/useUpdateQueries'
import { logger } from '../lib/logger'
import { notifications } from '../lib/notifications'
import { queryKeys } from '../lib/queryKeys'
import {
  DESKTOP_UPDATE_EVENT,
  DESKTOP_UPDATE_RECONNECT_EVENT,
  DESKTOP_UPDATE_RESET_EVENT,
  desktopService,
} from '../services/desktop'
import type { UpdateStatus } from '../types/updater'
import { isDesktopTauri } from '../utils/platform'
import { StandardModal } from './common/StandardModal'
import { LiveStatus } from './forms/LiveStatus'

export function DesktopUpdateBridge(): ReactNode {
  return isDesktopTauri() ? <NativeDesktopUpdateBridge /> : null
}

function NativeDesktopUpdateBridge(): ReactNode {
  const client = useQueryClient()
  const status = useUpdateStatus().data
  const connection = useUpdateConnection()
  const notices = useRef(new Set<string>())

  useEffect(() => {
    let active = true
    let generation = 0
    let subscription = 0
    let remove: (() => void) | undefined
    let connectionState: UpdateConnection = 'connecting'
    const setListening = (next: UpdateConnection) => {
      if (active) {
        connectionState = next
        client.setQueryData(queryKeys.updates.connection(), next)
      }
    }
    const refresh = () => {
      const requestGeneration = generation
      void desktopService
        .getUpdateStatus()
        .then((snapshot) => {
          if (active && requestGeneration === generation) {
            applyUpdateStatus(client, snapshot)
          }
        })
        .catch((error: unknown) => logger.warn('DesktopUpdates', "Couldn't read update status", error))
    }
    const reset = () => {
      generation += 1
      client.setQueryData(queryKeys.updates.connection(), connectionState)
      refresh()
    }
    const focus = () => {
      if (document.visibilityState !== 'hidden') {
        refresh()
      }
    }

    const reconnect = () => {
      const currentSubscription = ++subscription
      remove?.()
      remove = undefined
      setListening('connecting')
      void listen<UpdateStatus>(DESKTOP_UPDATE_EVENT, ({ payload }) => {
        if (active && currentSubscription === subscription) {
          applyUpdateStatus(client, payload)
        }
      })
        .then((unlisten) => {
          if (!active || currentSubscription !== subscription) {
            unlisten()
            return
          }
          remove = unlisten
          setListening('connected')
          refresh()
        })
        .catch((error: unknown) => {
          if (active && currentSubscription === subscription) {
            logger.warn('DesktopUpdates', "Couldn't receive update progress", error)
            setListening('failed')
          }
        })
    }
    reconnect()
    window.addEventListener('focus', focus)
    document.addEventListener('visibilitychange', focus)
    window.addEventListener(DESKTOP_UPDATE_RESET_EVENT, reset)
    window.addEventListener(DESKTOP_UPDATE_RECONNECT_EVENT, reconnect)

    return () => {
      active = false
      remove?.()
      window.removeEventListener('focus', focus)
      document.removeEventListener('visibilitychange', focus)
      window.removeEventListener(DESKTOP_UPDATE_RESET_EVENT, reset)
      window.removeEventListener(DESKTOP_UPDATE_RECONNECT_EVENT, reconnect)
    }
  }, [client])
  useEffect(() => {
    const notice = status?.notice
    if (!notice) {
      return
    }
    const key = `${notice.version}:${notice.outcome}`
    if (notices.current.has(key)) {
      return
    }
    notices.current.add(key)
    notifications.show({
      title: notice.outcome === 'updated' ? `Updated to ${notice.version}` : 'Update incomplete',
      message: notice.outcome === 'updated' ? 'Pipedash is ready.' : 'The update did not finish. You can retry.',
      color: notice.outcome === 'updated' ? 'green' : 'orange',
    })
    void desktopService
      .acknowledgeUpdateNotice(notice.version)
      .then((snapshot) => applyUpdateStatus(client, snapshot))
      .catch((error: unknown) => logger.warn('DesktopUpdates', "Couldn't acknowledge update notice", error))
  }, [client, status?.notice])

  const preparing = status?.phase === 'preparing'
  const busy = preparing || status?.phase === 'installing' || status?.phase === 'restarting'
  const message = preparing
    ? 'Waiting for current operations to finish'
    : status?.phase === 'installing'
      ? 'Installing update. Keep Pipedash open.'
      : 'Restarting Pipedash'

  return (
    <>
      {connection.state === 'failed' && (
        <Box pos="fixed" bottom="md" right="md" maw="min(420px, calc(100vw - 32px))" style={{ zIndex: 500 }}>
          <Alert color="orange" title="Couldn't receive update progress" role="alert">
            <Stack gap="sm">
              <Text size="sm">Reconnect before installing an update.</Text>
              <Button variant="light" onClick={connection.retryConnection}>
                Retry connection
              </Button>
            </Stack>
          </Alert>
        </Box>
      )}
      <StandardModal opened={busy} onClose={() => undefined} closable={false} size="md" title="Updating Pipedash">
        <Stack gap="md" align="center" py="md">
          <Loader aria-hidden />
          <Text ta="center">{message}</Text>
          <LiveStatus message={busy ? message : ''} />
        </Stack>
      </StandardModal>
    </>
  )
}
