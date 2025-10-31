import React, { useEffect, useState } from 'react'

import { ActionIcon, Badge, Box, Divider, Group, Image, Menu, Title, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconChevronDown, IconChevronsRight, IconRefresh, IconSettings } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { useRefresh } from '../../hooks/useRefresh'
import { platform } from '../../utils/platform'
import { WindowControls } from '../common/WindowControls'

interface HeaderProps {
  onRefreshCurrent?: () => void;
  onRefreshAll?: () => void;
  onToggleNavbar?: () => void;
  navbarOpened?: boolean;
  onOpenSettings?: () => void;
}

export const Header = React.memo(({
  onRefreshCurrent,
  onRefreshAll,
  onToggleNavbar,
  navbarOpened = false,
  onOpenSettings,
}: HeaderProps) => {
  const { mode } = useRefresh()
  const [loading, setLoading] = useState(false)
  const [isMacOS, setIsMacOS] = useState<boolean | null>(null)
  const isMobile = useIsMobile()

  // Detect platform on mount
  useEffect(() => {
    platform()
      .then((p) => setIsMacOS(p === 'macos'))
      .catch((err) => {
        console.error('Error detecting platform:', err)
        setIsMacOS(false)
      })
  }, [])

  const handleRefreshCurrent = async () => {
    setLoading(true)
    try {
      if (onRefreshCurrent) {
        await onRefreshCurrent()
      }
    } catch (error: any) {
      console.error('[Header] Failed to refresh:', error)
      const errorMsg = error?.error || error?.message || 'Failed to refresh'


      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const handleRefreshAll = async () => {
    setLoading(true)
    try {
      if (onRefreshAll) {
        await onRefreshAll()
      }
    } catch (error: any) {
      console.error('[Header] Failed to refresh all:', error)
      const errorMsg = error?.error || error?.message || 'Failed to refresh'


      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  if (isMacOS === null) {
    return <Box h="100%" />
  }

  return (
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
      {isMacOS && !isMobile && (
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
        {onToggleNavbar && isMobile && (
          <Tooltip label={navbarOpened ? 'Hide sidebar' : 'Show sidebar'} position="right" withArrow>
            <ActionIcon
              onClick={onToggleNavbar}
              size="lg"
              variant="subtle"
              color="gray"
            >
              <IconChevronsRight
                size={18}
                style={{
                  transform: navbarOpened ? 'rotate(180deg)' : 'rotate(0deg)',
                  transition: 'transform 200ms ease',
                }}
              />
            </ActionIcon>
          </Tooltip>
        )}

        <Group gap={isMobile ? 6 : 6} align="center">
          <Image src="/app-icon.png" alt="Pipedash" h={isMobile ? 26 : 28} w={isMobile ? 26 : 28} fit="contain" />
          <Title order={3} size="h3" fw={600} style={{ letterSpacing: '-0.02em', lineHeight: 1 }}>
            Pipedash
          </Title>
        </Group>

        <Box style={{ flex: 1 }} />

        <Group gap="sm">
          <Badge
            color={mode === 'active' ? 'blue' : 'gray'}
            variant="dot"
            size="md"
            hiddenFrom="xs"
            visibleFrom="sm"
          >
            {mode === 'active' ? 'Auto-refresh' : 'Idle'}
          </Badge>

          {onOpenSettings && (
            <Tooltip label="Settings" position="bottom">
              <ActionIcon
                variant="subtle"
                size="lg"
                onClick={onOpenSettings}
                color="gray"
              >
                <IconSettings size={18} />
              </ActionIcon>
            </Tooltip>
          )}

          {isMobile ? (
            <Tooltip label="Refresh" position="bottom">
              <ActionIcon
                variant="subtle"
                size="lg"
                onClick={handleRefreshCurrent}
                disabled={loading}
                color="gray"
              >
                <IconRefresh
                  size={18}
                  style={{
                    animation: loading ? 'spin 1s linear infinite' : 'none',
                    color: 'currentColor',
                  }}
                />
              </ActionIcon>
            </Tooltip>
          ) : (
            <Group gap={0}>
              <Tooltip label="Refresh current view" position="bottom">
                <ActionIcon
                  variant="subtle"
                  size="lg"
                  onClick={handleRefreshCurrent}
                  disabled={loading}
                  color="gray"
                  style={{ borderTopRightRadius: 0, borderBottomRightRadius: 0 }}
                >
                  <IconRefresh
                    size={18}
                    style={{
                      animation: loading ? 'spin 1s linear infinite' : 'none',
                      color: 'currentColor',
                    }}
                  />
                </ActionIcon>
              </Tooltip>

              <Menu shadow="md" width={200} position="bottom-end">
                <Menu.Target>
                  <Tooltip label="More refresh options" position="bottom">
                    <ActionIcon
                      variant="subtle"
                      size="lg"
                      color="gray"
                      disabled={loading}
                      style={{ borderTopLeftRadius: 0, borderBottomLeftRadius: 0 }}
                    >
                      <IconChevronDown size={12} />
                    </ActionIcon>
                  </Tooltip>
                </Menu.Target>

                <Menu.Dropdown>
                  <Menu.Item
                    leftSection={<IconRefresh size={16} />}
                    onClick={handleRefreshAll}
                    disabled={loading}
                  >
                    Refresh All Providers
                  </Menu.Item>
                </Menu.Dropdown>
              </Menu>
            </Group>
          )}
        </Group>
      </Box>

      {!isMacOS && !isMobile && (
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
      `}</style>
    </Box>
  )
})
