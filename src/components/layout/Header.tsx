import { ActionIcon, Box, Divider, Group, Kbd, Text, Tooltip, UnstyledButton } from '@mantine/core'
import {
  IconArrowLeft,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconLock,
  IconLockOpen,
  IconSettings,
} from '@tabler/icons-react'
import { type ReactNode, useEffect, useState } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { NotificationCenter } from '../../features/notifications/components/NotificationCenter'
import { useAppShortcuts } from '../../hooks/useAppShortcuts'
import { useIsMobile } from '../../hooks/useIsMobile'
import { useRefresh } from '../../hooks/useRefresh'
import { logger } from '../../lib/logger'
import { shortcutLabel } from '../../lib/shortcuts'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { useLockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { lockMeansSomething } from '../../utils/appLock'
import { isDesktopTauri, platform } from '../../utils/platform'
import { AppLogo } from '../atoms/AppLogo'
import { ConnectionStatus } from '../common/ConnectionStatus'
import { RefreshButton } from '../common/RefreshButton'
import { WindowControls } from '../common/WindowControls'

interface HeaderProps {
  onToggleNavbar?: (() => void) | undefined
  navbarOpened?: boolean | undefined
  onOpenSettings?: (() => void) | undefined
}

export function Header({ onToggleNavbar, navbarOpened = false, onOpenSettings }: HeaderProps): ReactNode {
  useRefresh()
  const refreshAll = useRefreshAll()
  const [isMacOS, setIsMacOS] = useState<boolean | null>(null)
  const { isMobile } = useIsMobile()
  const settingsOpen = useLocation().pathname.startsWith('/settings')

  const { data: vaultStatus } = useVaultStatus()
  const lockMutation = useLockVault()
  const isVaultUnlocked = vaultStatus?.is_unlocked ?? false
  const showLock = vaultStatus ? lockMeansSomething(vaultStatus) : false
  const canLock = isVaultUnlocked && showLock

  const showWindowChrome = isDesktopTauri()

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

  const handleLockVault = (): void => {
    if (canLock && !lockMutation.isPending) {
      lockMutation.mutate()
    }
  }

  const handleRefresh = (): void => {
    if (!refreshAll.isPending) {
      refreshAll.mutate()
    }
  }

  useAppShortcuts({
    onSettings: onOpenSettings,
    onSidebar: onToggleNavbar,
    onRefresh: handleRefresh,
    onLock: canLock ? handleLockVault : undefined,
  })

  if (isMacOS === null) {
    return <Box h="100%" />
  }

  const sidebarLabel = navbarOpened ? 'Collapse sidebar' : 'Expand sidebar'
  const lockLabel = canLock ? 'Lock vault' : 'Vault is locked'
  const settingsLabel = settingsOpen ? 'Back to Pipelines' : 'Settings'
  const commandTooltip = (label: string, combo?: string) => (
    <Group gap="3xs" wrap="nowrap">
      <span>{label}</span>
      {combo && <Kbd size="xs">{shortcutLabel(combo)}</Kbd>}
    </Group>
  )

  const sidebarToggle = onToggleNavbar && (
    <Tooltip label={commandTooltip(sidebarLabel, 'mod+\\')} position="bottom">
      <ActionIcon onClick={onToggleNavbar} size="lg" aria-label={sidebarLabel}>
        {navbarOpened ? (
          <IconLayoutSidebarLeftCollapse size={20} aria-hidden />
        ) : (
          <IconLayoutSidebarLeftExpand size={20} aria-hidden />
        )}
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
        pt="var(--safe-top)"
        style={{ WebkitUserSelect: 'none', position: 'relative' }}
      >
        {showWindowChrome && isMacOS && (
          <Group gap={0} wrap="nowrap" h="100%">
            <WindowControls />
            <Divider orientation="vertical" mx="xs" h="50%" />
          </Group>
        )}

        <Group flex={1} h="100%" gap="sm" wrap="nowrap" px={isMobile ? 'xs' : 'md'}>
          <Group gap="xs" wrap="nowrap">
            {isMobile && sidebarToggle}

            <UnstyledButton component={Link} to="/pipelines" aria-label="Pipedash home" className="interactive-surface">
              <Group gap="xs" wrap="nowrap">
                <AppLogo size={isMobile ? 26 : 28} />
                <Text fw={600} lh={1} fz="var(--mantine-h3-font-size)">
                  Pipedash
                </Text>
              </Group>
            </UnstyledButton>

            {!isMobile && sidebarToggle}
          </Group>
          <Box
            flex={1}
            h="100%"
            style={{ alignSelf: 'stretch' }}
            data-tauri-drag-region={showWindowChrome || undefined}
          />

          <Group gap="sm" wrap="nowrap">
            {showLock && (
              <Tooltip
                label={commandTooltip(lockLabel, showWindowChrome && canLock ? 'mod+L' : undefined)}
                position="bottom"
              >
                <ActionIcon
                  size="lg"
                  onClick={canLock ? handleLockVault : undefined}
                  disabled={!canLock}
                  aria-label={lockLabel}
                >
                  {isVaultUnlocked ? <IconLockOpen size={20} aria-hidden /> : <IconLock size={20} aria-hidden />}
                </ActionIcon>
              </Tooltip>
            )}

            <NotificationCenter />

            <Tooltip label={commandTooltip('Refresh all', showWindowChrome ? 'mod+R' : undefined)} position="bottom">
              <RefreshButton
                size="lg"
                iconSize={20}
                pending={refreshAll.isPending}
                onClick={handleRefresh}
                aria-label="Refresh all"
              />
            </Tooltip>

            {onOpenSettings && (
              <Tooltip label={commandTooltip(settingsLabel, 'mod+,')} position="bottom">
                <ActionIcon size="lg" onClick={onOpenSettings} aria-label={settingsLabel}>
                  {settingsOpen ? <IconArrowLeft size={20} aria-hidden /> : <IconSettings size={20} aria-hidden />}
                </ActionIcon>
              </Tooltip>
            )}
          </Group>
        </Group>

        {showWindowChrome && !isMacOS && (
          <Group gap={0} wrap="nowrap" h="100%">
            <Divider orientation="vertical" mx="sm" h="50%" />
            <WindowControls />
          </Group>
        )}
      </Group>
    </>
  )
}
