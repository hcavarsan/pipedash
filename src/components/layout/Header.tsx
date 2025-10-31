import React, { useEffect, useState } from 'react'

import { ActionIcon, Badge, Box, Divider, Group, Image, Title, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconChevronsRight, IconRefresh, IconSettings } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { useRefresh } from '../../hooks/useRefresh'
import { platform } from '../../utils/platform'
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
  const { mode } = useRefresh()
  const [isMacOS, setIsMacOS] = useState<boolean | null>(null)
  const isMobile = useIsMobile()

  useEffect(() => {
    platform()
      .then((p) => setIsMacOS(p === 'macos'))
      .catch((err) => {
        console.error('Error detecting platform:', err)
        setIsMacOS(false)
      })
  }, [])

  const handleRefresh = async () => {
    try {
      if (onRefreshAll) {
        await onRefreshAll()
      }
    } catch (error: any) {
      console.error('[Header] Failed to refresh:', error)
      const errorMsg = error?.error || error?.message || 'Failed to refresh'


      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
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

          <Tooltip label="Refresh all" position="bottom">
            <ActionIcon
              variant="subtle"
              size="xl"
              onClick={handleRefresh}
              disabled={refreshing}
              color="gray"
              style={{
                backgroundColor: 'transparent',
                cursor: refreshing ? 'not-allowed' : 'pointer',
              }}
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
