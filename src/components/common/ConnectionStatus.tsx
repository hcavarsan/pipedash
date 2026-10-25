import { Alert, Button, Group, Text, Tooltip } from '@mantine/core'
import { useNetwork } from '@mantine/hooks'
import { IconLock, IconRefresh, IconWifiOff } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useEffect, useRef, useState } from 'react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { useVaultStatus } from '@/queries/useVaultQueries'
import { isTauri, service } from '@/services'
import { WS_EVENTS, wsClient } from '@/services/websocket'

import classes from './ConnectionStatus.module.css'

interface ShellBanner {
  icon: ReactNode
  title: string
  detail: string
  reconnect?: boolean
}

interface ConnectionState {
  status: 'connected' | 'disconnected' | 'reconnecting'
  reconnectAttempts: number
}

const ALERT_POSITION = {
  position: 'fixed',
  top: 'calc(60px + var(--safe-top))',
  right: 'calc(16px + var(--safe-right))',
  zIndex: 1000,
} as const

export function ConnectionStatus() {
  const [connectionState, setConnectionState] = useState<ConnectionState>({
    status: 'connected',
    reconnectAttempts: 0,
  })

  const { data: vaultStatus } = useVaultStatus()
  const queryClient = useQueryClient()
  const { online } = useNetwork()
  const wasOnline = useRef(online)
  const mobileShell = useMobileShell()

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

  if (mobileShell) {
    let banner: ShellBanner | null = null

    if (vaultLocked) {
      banner = { icon: <IconLock size={14} />, title: 'Vault locked', detail: 'Token storage is locked' }
    } else if (isTauri() ? !online : connectionState.status !== 'connected') {
      const reconnecting = connectionState.status === 'reconnecting'

      banner = isTauri()
        ? { icon: <IconWifiOff size={14} />, title: "You're offline", detail: 'Refreshes when back online' }
        : {
            icon: <IconWifiOff size={14} />,
            title: reconnecting ? 'Reconnecting' : 'Disconnected',
            detail: reconnecting ? `Attempt ${connectionState.reconnectAttempts}` : 'Live updates unavailable',
            reconnect: true,
          }
    }

    return banner ? (
      <div className={classes.banner} role="status">
        <span className={classes.icon} aria-hidden>
          {banner.icon}
        </span>
        <span className={classes.text}>
          {banner.title}
          <span className={classes.detail}> · {banner.detail}</span>
        </span>
        {banner.reconnect && (
          <button type="button" className={classes.action} onClick={handleManualReconnect}>
            Reconnect
          </button>
        )}
      </div>
    ) : null
  }

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
            style={{ ...ALERT_POSITION, maxWidth: 300 }}
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
          style={{ ...ALERT_POSITION, maxWidth: 300 }}
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
        style={{ ...ALERT_POSITION, maxWidth: 400 }}
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
      style={{ ...ALERT_POSITION, maxWidth: 400 }}
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
