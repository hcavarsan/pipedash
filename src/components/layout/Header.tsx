import { ActionIcon, Box, Divider, Group, Kbd, Title, Tooltip, UnstyledButton } from '@mantine/core'
import { useHotkeys } from '@mantine/hooks'
import {
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconLock,
  IconLockOpen,
  IconRefresh,
  IconSettings,
} from '@tabler/icons-react'
import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useRefresh } from '../../hooks/useRefresh'
import { logger } from '../../lib/logger'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { useLockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { isTauri } from '../../services'
import { platform } from '../../utils/platform'
import { AppLogo } from '../atoms/AppLogo'
import { ConnectionStatus } from '../common/ConnectionStatus'
import { WindowControls } from '../common/WindowControls'

interface HeaderProps {
  onToggleNavbar?: () => void
  navbarOpened?: boolean
  onOpenSettings?: () => void
}

export const Header = React.memo(({ onToggleNavbar, navbarOpened = false, onOpenSettings }: HeaderProps) => {
  useRefresh()
  const refreshAll = useRefreshAll()
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
        logger.error('Header', 'Error detecting platform', err)
        if (isMounted) {
          setIsMacOS(false)
        }
      })

    return () => {
      isMounted = false
    }
  }, [])

  const handleLockVault = useCallback(() => {
    if (canLock && !lockMutation.isPending) {
      lockMutation.mutate()
    }
  }, [canLock, lockMutation])

  useHotkeys([
    [
      'mod+L',
      () => {
        if (canLock) {
          handleLockVault()
        }
      },
      { preventDefault: true },
    ],
  ])

  if (isMacOS === null) {
    return <Box h="100%" />
  }

  const sidebarLabel = navbarOpened ? 'Collapse sidebar' : 'Expand sidebar'
  const lockLabel = canLock ? 'Lock vault' : 'Vault is locked'

  const sidebarToggle = onToggleNavbar && (
    <Tooltip label={sidebarLabel} position="bottom">
      <ActionIcon onClick={onToggleNavbar} size="lg" variant="subtle" color="gray" aria-label={sidebarLabel}>
        {navbarOpened ? <IconLayoutSidebarLeftCollapse size={20} /> : <IconLayoutSidebarLeftExpand size={20} />}
      </ActionIcon>
    </Tooltip>
  )

  return (
    <>
      <ConnectionStatus />
      <Group
        h="100%"
        gap={0}
        wrap="nowrap"
        data-tauri-drag-region
        pt={isMobile ? 'env(safe-area-inset-top)' : 0}
        style={{ WebkitUserSelect: 'none', position: 'relative' }}
      >
        {isRunningInTauri && isMacOS && !isMobile && (
          <Group gap={0} wrap="nowrap" h="100%">
            <WindowControls />
            <Divider orientation="vertical" mx="xs" h="50%" />
          </Group>
        )}

        <Group flex={1} gap="sm" wrap="nowrap" justify="space-between" px={isMobile ? 'xs' : 'md'}>
          <Group gap="xs" wrap="nowrap">
            {isMobile && sidebarToggle}

            <UnstyledButton component={Link} to="/pipelines" aria-label="Pipedash home">
              <Group gap="xs" wrap="nowrap">
                <AppLogo size={isMobile ? 26 : 28} />
                <Title order={3} fw={600} lh={1}>
                  Pipedash
                </Title>
              </Group>
            </UnstyledButton>

            {!isMobile && sidebarToggle}
          </Group>

          <Group gap="sm" wrap="nowrap">
            {requiresPassword && (
              <Tooltip
                label={
                  canLock ? (
                    <Group gap={4} wrap="nowrap">
                      <span>{lockLabel}</span>
                      <Kbd size="xs">{isMacOS ? '⌘' : 'Ctrl'}</Kbd>
                      <Kbd size="xs">L</Kbd>
                    </Group>
                  ) : (
                    lockLabel
                  )
                }
                position="bottom"
              >
                <ActionIcon
                  variant="subtle"
                  size="lg"
                  onClick={canLock ? handleLockVault : undefined}
                  disabled={!canLock}
                  color="gray"
                  aria-label={lockLabel}
                >
                  {isVaultUnlocked ? <IconLockOpen size={20} /> : <IconLock size={20} />}
                </ActionIcon>
              </Tooltip>
            )}

            <Tooltip label="Refresh all" position="bottom">
              <ActionIcon
                variant="subtle"
                size="lg"
                onClick={() => refreshAll.mutate()}
                loading={refreshAll.isPending}
                color="gray"
                aria-label="Refresh all"
              >
                <IconRefresh size={20} />
              </ActionIcon>
            </Tooltip>

            {onOpenSettings && (
              <Tooltip label="Settings" position="bottom">
                <ActionIcon variant="subtle" size="lg" onClick={onOpenSettings} color="gray" aria-label="Settings">
                  <IconSettings size={20} />
                </ActionIcon>
              </Tooltip>
            )}
          </Group>
        </Group>

        {isRunningInTauri && !isMacOS && !isMobile && (
          <Group gap={0} wrap="nowrap" h="100%">
            <Divider orientation="vertical" mx="sm" h="50%" />
            <WindowControls />
          </Group>
        )}
      </Group>
    </>
  )
})
