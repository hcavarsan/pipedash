import {
  ActionIcon,
  Box,
  Button,
  Group,
  Indicator,
  Popover,
  ScrollArea,
  Stack,
  Text,
  Tooltip,
  UnstyledButton,
  VisuallyHidden,
} from '@mantine/core'
import { IconBell, IconBellCheck } from '@tabler/icons-react'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef, useState } from 'react'

import { useNow } from '../../hooks/useNow'
import { logger } from '../../lib/logger'
import { useProviders } from '../../queries/useProvidersQueries'
import { isTauri } from '../../services'
import {
  type CenterNotification,
  type CenterNotificationTone,
  useNotificationCenterStore,
} from '../../stores/notificationCenterStore'
import { HEALTH_META } from '../../theme/status'
import type { HealthLevel } from '../../types'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { openNotificationDetails, SYSTEM_NOTIFICATION_CLICKED_EVENT } from '../../utils/systemNotifications'

import { notificationIcon, notificationLabel } from './labels'
import { NotificationDetailsModal } from './NotificationDetailsModal'
import { ProviderStatusModal } from './ProviderStatusModal'

const DOT_COLORS: Array<[CenterNotificationTone, string]> = [
  ['danger', 'red'],
  ['warning', 'orange'],
  ['success', 'blue'],
]

function unreadDotColor(items: CenterNotification[]): string | null {
  const unread = items.filter((item) => !item.read)

  return DOT_COLORS.find(([tone]) => unread.some((item) => item.tone === tone))?.[1] ?? null
}

function Row({ item, now, onClick }: { item: CenterNotification; now: number; onClick: () => void }) {
  const RowIcon = notificationIcon(item)

  return (
    <UnstyledButton
      className="interactive-surface"
      px="sm"
      py={8}
      onClick={onClick}
      style={{ borderRadius: 'var(--mantine-radius-sm)' }}
    >
      <Group gap="sm" wrap="nowrap" align="flex-start">
        <Box c="dimmed" pt={2} style={{ display: 'flex' }}>
          <RowIcon size={16} aria-hidden />
        </Box>
        <Stack gap={0} style={{ flex: 1, minWidth: 0 }}>
          <Text size="sm" fw={item.read ? 500 : 600} truncate>
            {item.title}
          </Text>
          <Text size="xs" c="dimmed" lineClamp={2}>
            {notificationLabel(item)} · {formatRelativeTime(item.at, now)}
          </Text>
        </Stack>
        {!item.read && (
          <Box w={6} h={6} mt={7} bg="blue" style={{ borderRadius: '50%', flexShrink: 0 }}>
            <VisuallyHidden>Unread</VisuallyHidden>
          </Box>
        )}
      </Group>
    </UnstyledButton>
  )
}

export function NotificationCenter() {
  const { data: providers } = useProviders()
  const items = useNotificationCenterStore((state) => state.items)
  const open = useNotificationCenterStore((state) => state.open)
  const markAllRead = useNotificationCenterStore((state) => state.markAllRead)
  const clear = useNotificationCenterStore((state) => state.clear)
  const [opened, setOpened] = useState(false)
  const [announcement, setAnnouncement] = useState('')
  const previousLevels = useRef<Map<number, HealthLevel> | null>(null)
  const now = useNow(30_000)

  const unread = items.filter((item) => !item.read).length
  const dotColor = unreadDotColor(items)

  useEffect(() => {
    if (!isTauri()) {
      return
    }

    let active = true
    let unlisten: (() => void) | undefined

    listen<string>(SYSTEM_NOTIFICATION_CLICKED_EVENT, (event) => openNotificationDetails(event.payload))
      .then((stop) => {
        if (active) {
          unlisten = stop
        } else {
          stop()
        }
      })
      .catch((error: unknown) => logger.warn('NotificationCenter', "Couldn't listen for notification clicks", error))

    return () => {
      active = false
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    if (!providers) {
      return
    }

    const previous = previousLevels.current

    previousLevels.current = new Map(providers.map((provider) => [provider.id, provider.health.level]))

    if (!previous) {
      return
    }

    const changes = providers
      .filter((provider) => {
        const before = previous.get(provider.id)

        return before !== undefined && before !== provider.health.level
      })
      .map((provider) => `${provider.name} status: ${HEALTH_META[provider.health.level].label}`)

    if (changes.length > 0) {
      setAnnouncement(changes.join('. '))
    }
  }, [providers])

  const toggle = (next: boolean) => {
    if (!next) {
      markAllRead()
    }
    setOpened(next)
  }

  const label = unread > 0 ? `Notifications, ${unread} unread` : 'Notifications'

  return (
    <>
      <VisuallyHidden role="status">{announcement}</VisuallyHidden>
      <Indicator color={dotColor ?? 'gray'} disabled={!dotColor} size={8} offset={7} withBorder>
        <Popover opened={opened} onChange={toggle} position="bottom-end" width={380} shadow="md" trapFocus returnFocus>
          <Popover.Target>
            <Tooltip label="Notifications" position="bottom" disabled={opened}>
              <ActionIcon variant="subtle" size="lg" color="gray" aria-label={label} onClick={() => toggle(!opened)}>
                <IconBell size={20} />
              </ActionIcon>
            </Tooltip>
          </Popover.Target>

          <Popover.Dropdown p={0} maw="calc(100vw - 16px)">
            <Group justify="space-between" px="md" py="sm">
              <Text fw={600} size="sm">
                Notifications
              </Text>
              <Group gap={4}>
                {unread > 0 && (
                  <Button size="compact-xs" variant="subtle" color="gray" onClick={markAllRead}>
                    Mark all read
                  </Button>
                )}
                {items.length > 0 && (
                  <Button size="compact-xs" variant="subtle" color="gray" onClick={clear}>
                    Clear
                  </Button>
                )}
              </Group>
            </Group>

            {items.length === 0 ? (
              <Stack align="center" gap={4} px="md" pb="lg" pt="xs">
                <Box c="dimmed">
                  <IconBellCheck size={24} aria-hidden />
                </Box>
                <Text size="sm" fw={500}>
                  You're all caught up
                </Text>
                <Text size="xs" c="dimmed" ta="center">
                  Provider outages and pipeline failures show up here.
                </Text>
              </Stack>
            ) : (
              <ScrollArea.Autosize mah={440} type="auto">
                <Stack gap={2} px={6} pb={6}>
                  {items.map((item) => (
                    <Row
                      key={item.id}
                      item={item}
                      now={now}
                      onClick={() => {
                        toggle(false)
                        open({ type: 'notification', id: item.id })
                      }}
                    />
                  ))}
                </Stack>
              </ScrollArea.Autosize>
            )}
          </Popover.Dropdown>
        </Popover>
      </Indicator>
      <NotificationDetailsModal />
      <ProviderStatusModal />
    </>
  )
}
