import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { ActionIcon, Badge, Box, Divider, Group, Image, Kbd, Title, Tooltip } from '@mantine/core'
import { useHotkeys } from '@mantine/hooks'
import { notifications } from '@mantine/notifications'
import { IconLayoutSidebarLeftCollapse, IconLayoutSidebarLeftExpand, IconLock, IconLockOpen, IconRefresh, IconSettings } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useRefresh } from '../../hooks/useRefresh'
import { useLockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { isTauri } from '../../services'
import { clearApiToken } from '../../services/auth'
import { platform } from '../../utils/platform'
import { ConnectionStatus } from '../common/ConnectionStatus'
import { WindowControls } from '../common/WindowControls'

interface HeaderProps {
  onRefreshAll?: () => void;
  onToggleNavbar?: () => void;
  navbarOpened?: boolean;
  onOpenSettings?: () => void;
  refreshing?: boolean;
}

export const Header = React.memo(({
  onRefreshAll,
  onToggleNavbar,
  navbarOpened = false,
  onOpenSettings,
  refreshing = false,
}: HeaderProps) => {
  const navigate = useNavigate()
  const { mode } = useRefresh()
  const [isMacOS, setIsMacOS] = useState<boolean | null>(null)
  const { isMobile } = useIsMobile()

  const { data: vaultStatus } = useVaultStatus()
  const lockMutation = useLockVault()
  const isVaultUnlocked = vaultStatus?.is_unlocked ?? false
  const requiresPassword = vaultStatus?.requires_password ?? false
  const canLock = isVaultUnlocked && requiresPassword

  const isRunningInTauri = useMemo(() => isTauri(), [])

  useEffect(() => {
    let isMounted = true

    platform()
      .then((p) => {
        if (isMounted) {
setIsMacOS(p === 'macos')
}
      })
      .catch((err) => {
        console.error('Error detecting platform:', err)
        if (isMounted) {
setIsMacOS(false)
}
      })

    return () => {
      isMounted = false
    }
  }, [])

  const handleLogoClick = useCallback(() => {
    navigate('/pipelines')
  }, [navigate])

  const handleRefresh = async () => {
    try {
      if (onRefreshAll) {
        await onRefreshAll()
      }
    } catch (error: unknown) {
      console.error('[Header] Failed to refresh:', error)
      const errorMsg = error instanceof Error ? error.message : 'Failed to refresh'

      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    }
  }

  const handleLockVault = useCallback(async () => {
    if (!canLock || lockMutation.isPending) {
      return
    }

    try {
      await lockMutation.mutateAsync()
      clearApiToken()
    } catch (error) {
      console.error('[Header] Failed to lock vault:', error)
      notifications.show({
        title: 'Error',
        message: 'Failed to lock vault',
        color: 'red',
      })
    }
  }, [canLock, lockMutation])

  useHotkeys([
    ['mod+L', () => {
      if (canLock) {
        handleLockVault()
      }
    }, { preventDefault: true }],
  ])

  if (isMacOS === null) {
    return <Box h="100%" />
  }

  return (
    <>
      <ConnectionStatus />
      <Box
        h="100%"
        data-tauri-drag-region
        style={{
          WebkitUserSelect: 'none',
          position: 'relative',
          paddingTop: isMobile ? 'env(safe-area-inset-top)' : 0,
          display: 'flex',
          alignItems: 'center',
          gap: 0,
        }}
      >
      {isRunningInTauri && isMacOS && !isMobile && (
        <Box style={{ display: 'flex', alignItems: 'center' }}>
          <WindowControls />
          <Divider orientation="vertical" mx={8} style={{ height: '50%' }} />
        </Box>
      )}

      <Box
        style={{
          flex: 1,
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--mantine-spacing-sm)',
          paddingLeft: isMobile ? 8 : 4,
          paddingRight: isMobile ? 8 : 16,
        }}
      >
        <Group gap={isMobile ? 6 : 8} align="center" wrap="nowrap">
          {onToggleNavbar && isMobile && (
            <Tooltip label={navbarOpened ? 'Hide sidebar' : 'Show sidebar'} position="right" withArrow>
              <ActionIcon
                onClick={onToggleNavbar}
                size="lg"
                variant="subtle"
                color="gray"
                className="header-sidebar-toggle"
              >
                {navbarOpened ? (
                  <IconLayoutSidebarLeftCollapse size={20} />
                ) : (
                  <IconLayoutSidebarLeftExpand size={20} />
                )}
              </ActionIcon>
            </Tooltip>
          )}

          <Box onClick={handleLogoClick} style={{ cursor: 'pointer', marginLeft: isMobile ? 0 : 12 }}>
            <Group gap={6} align="center">
              <Image src="/app-icon.png" alt="Pipedash" h={isMobile ? 26 : 28} w={isMobile ? 26 : 28} fit="contain" />
              <Title order={3} size="h3" fw={600} style={{ letterSpacing: '-0.02em', lineHeight: 1 }}>
                Pipedash
              </Title>
            </Group>
          </Box>

          {onToggleNavbar && !isMobile && (
            <Tooltip label={navbarOpened ? 'Hide sidebar' : 'Show sidebar'} position="bottom" withArrow>
              <ActionIcon
                onClick={onToggleNavbar}
                size="md"
                variant="subtle"
                color="gray"
                className="header-sidebar-toggle"
              >
                {navbarOpened ? (
                  <IconLayoutSidebarLeftCollapse size={18} />
                ) : (
                  <IconLayoutSidebarLeftExpand size={18} />
                )}
              </ActionIcon>
            </Tooltip>
          )}
        </Group>

        <Box style={{ flex: 1 }} />

        <Group gap="sm">
          <Badge
            color={mode === 'active' ? 'blue' : 'gray'}
            variant="dot"
            size="md"
            fw={500}
            hiddenFrom="xs"
            visibleFrom="sm"
          >
            {mode === 'active' ? 'Auto-refresh' : 'Idle'}
          </Badge>

          {requiresPassword && (
            <Tooltip
              label={
                canLock ? (
                  <Group gap={4} wrap="nowrap">
                    <span>Lock vault</span>
                    <Kbd size="xs">{isMacOS ? '⌘' : 'Ctrl'}</Kbd>
                    <Kbd size="xs">L</Kbd>
                  </Group>
                ) : (
                  'Vault is locked'
                )
              }
              position="bottom"
            >
              <ActionIcon
                variant="subtle"
                size="xl"
                onClick={canLock ? handleLockVault : undefined}
                disabled={!canLock}
                color="gray"
              >
                {isVaultUnlocked ? <IconLockOpen size={22} /> : <IconLock size={22} />}
              </ActionIcon>
            </Tooltip>
          )}

          <Tooltip label="Refresh all" position="bottom">
            <ActionIcon
              variant="subtle"
              size="xl"
              onClick={handleRefresh}
              disabled={refreshing}
              color="gray"
            >
              <IconRefresh
                size={22}
                style={{
                  animation: refreshing ? 'spin 1s linear infinite' : 'none',
                  color: 'currentColor',
                }}
              />
            </ActionIcon>
          </Tooltip>

          {onOpenSettings && (
            <Tooltip label="Settings" position="bottom">
              <ActionIcon
                variant="subtle"
                size="xl"
                onClick={onOpenSettings}
                color="gray"
              >
                <IconSettings size={22} />
              </ActionIcon>
            </Tooltip>
          )}
        </Group>
      </Box>

      {isRunningInTauri && !isMacOS && !isMobile && (
        <Box style={{ display: 'flex', alignItems: 'center' }}>
          <Divider orientation="vertical" mx="sm" style={{ height: '50%' }} />
          <WindowControls />
        </Box>
      )}

      <style>{`
        @keyframes spin {
          from {
            transform: rotate(0deg);
          }
          to {
            transform: rotate(360deg);
          }
        }

        .header-sidebar-toggle {
          opacity: 0.5;
          transition: all 0.2s ease;
        }
        .header-sidebar-toggle:hover {
          opacity: 1;
          background-color: rgba(0, 0, 0, 0.05);
        }
        [data-mantine-color-scheme="dark"] .header-sidebar-toggle:hover {
          background-color: rgba(255, 255, 255, 0.05);
        }
      `}</style>
    </Box>
    </>
  )
})
