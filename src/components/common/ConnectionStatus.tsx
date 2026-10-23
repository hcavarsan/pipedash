import { Alert, Button, Group, Text, Tooltip } from '@mantine/core'
import { useNetwork } from '@mantine/hooks'
import { IconLock, IconRefresh, IconWifiOff } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'

import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { useVaultStatus } from '@/queries/useVaultQueries'
import { isTauri, service } from '@/services'
import { WS_EVENTS, wsClient } from '@/services/websocket'

interface ConnectionState {
  status: 'connected' | 'disconnected' | 'reconnecting'
  reconnectAttempts: number
}

export function ConnectionStatus() {
  const [connectionState, setConnectionState] = useState<ConnectionState>({
    status: 'connected',
    reconnectAttempts: 0,
  })

  const { data: vaultStatus } = useVaultStatus()
  const queryClient = useQueryClient()
  const { online } = useNetwork()
  const wasOnline = useRef(online)

  useEffect(() => {
    if (isTauri()) {
      return
    }

    const unlisten = wsClient.listen<ConnectionState>(WS_EVENTS.CONNECTION_STATUS, (state) => {
      setConnectionState(state)
    })

    return () => {
      unlisten()
    }
  }, [])

  useEffect(() => {
    const cameBackOnline = online && !wasOnline.current

    wasOnline.current = online

    if (!cameBackOnline) {
      return
    }

    const invalidate = () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.providers.all }),
        queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all }),
      ])

    if (isTauri()) {
      service
        .refreshAll()
        .catch((error) => logger.error('ConnectionStatus', 'Refresh after reconnect failed', error))
        .finally(invalidate)
    } else {
      wsClient.manualReconnect()
      invalidate()
    }
  }, [online, queryClient])

  const handleManualReconnect = () => {
    wsClient.manualReconnect()
  }

  const vaultLocked = vaultStatus?.requires_password && !vaultStatus?.is_unlocked

  if (isTauri()) {
    if (vaultLocked) {
      return (
        <Tooltip label="Vault is locked - some features may be unavailable">
          <Alert
            icon={<IconLock size={16} />}
            title="Vault locked"
            color="orange"
            variant="light"
            withCloseButton={false}
            style={{ position: 'fixed', top: 60, right: 16, zIndex: 1000, maxWidth: 300 }}
          >
            <Text size="sm">Token storage is locked</Text>
          </Alert>
        </Tooltip>
      )
    }

    if (!online) {
      return (
        <Alert
          icon={<IconWifiOff size={16} />}
          title="You're offline"
          color="orange"
          variant="light"
          withCloseButton={false}
          style={{ position: 'fixed', top: 60, right: 16, zIndex: 1000, maxWidth: 300 }}
        >
          <Text size="sm">Pipedash refreshes your providers once the connection is back.</Text>
        </Alert>
      )
    }

    return null
  }

  if (vaultLocked) {
    return (
      <Alert
        icon={<IconLock size={16} />}
        title="Vault locked"
        color="orange"
        variant="light"
        withCloseButton={false}
        style={{ position: 'fixed', top: 60, right: 16, zIndex: 1000, maxWidth: 400 }}
      >
        <Text size="sm">Token storage is locked. Set PIPEDASH_VAULT_PASSWORD or restart the app.</Text>
      </Alert>
    )
  }

  if (connectionState.status === 'connected') {
    return null
  }

  return (
    <Alert
      icon={<IconWifiOff size={16} />}
      title={connectionState.status === 'reconnecting' ? 'Reconnecting' : 'Disconnected'}
      color="orange"
      variant="light"
      withCloseButton={false}
      style={{ position: 'fixed', top: 60, right: 16, zIndex: 1000, maxWidth: 400 }}
    >
      <Group gap="xs" align="center">
        <Text size="sm">
          {connectionState.status === 'reconnecting'
            ? `Attempt ${connectionState.reconnectAttempts}`
            : 'Real-time updates unavailable'}
        </Text>
        <Button size="xs" variant="default" leftSection={<IconRefresh size={14} />} onClick={handleManualReconnect}>
          Reconnect
        </Button>
      </Group>
    </Alert>
  )
}
