import { useEffect, useState } from 'react'

import { Alert, Button, Group, Text, Tooltip } from '@mantine/core'
import { IconLock, IconRefresh, IconWifiOff } from '@tabler/icons-react'

import { useVaultStatus } from '@/queries/useVaultQueries'
import { isTauri } from '@/services'
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

  useEffect(() => {
    if (isTauri()) {
      return
    }

    const unlisten = wsClient.listen<ConnectionState>(
      WS_EVENTS.CONNECTION_STATUS,
      (state) => {
        setConnectionState(state)
      }
    )

    return () => {
      unlisten()
    }
  }, [])

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
            title="Vault Locked"
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
    
return null
  }

  if (vaultLocked) {
    return (
      <Alert
        icon={<IconLock size={16} />}
        title="Vault Locked"
        color="orange"
        variant="light"
        withCloseButton={false}
        style={{ position: 'fixed', top: 60, right: 16, zIndex: 1000, maxWidth: 400 }}
      >
        <Text size="sm">
          Token storage is locked. Set PIPEDASH_VAULT_PASSWORD or restart the app.
        </Text>
      </Alert>
    )
  }

  if (connectionState.status === 'connected') {
    return null
  }

  return (
    <Alert
      icon={<IconWifiOff size={16} />}
      title={connectionState.status === 'reconnecting' ? 'Reconnecting...' : 'Disconnected'}
      color={connectionState.status === 'reconnecting' ? 'yellow' : 'red'}
      variant="light"
      withCloseButton={false}
      style={{ position: 'fixed', top: 60, right: 16, zIndex: 1000, maxWidth: 400 }}
    >
      <Group gap="xs" align="center">
        <Text size="sm">
          {connectionState.status === 'reconnecting'
            ? `Attempt ${connectionState.reconnectAttempts}...`
            : 'Real-time updates unavailable'}
        </Text>
        <Button
          size="xs"
          variant="light"
          leftSection={<IconRefresh size={14} />}
          onClick={handleManualReconnect}
        >
          Reconnect
        </Button>
      </Group>
    </Alert>
  )
}
