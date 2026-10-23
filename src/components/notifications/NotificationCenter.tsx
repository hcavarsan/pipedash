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
import { type Icon, IconBell, IconBellCheck } from '@tabler/icons-react'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useMemo, useRef, useState } from 'react'

import { useNow } from '../../hooks/useNow'
import { logger } from '../../lib/logger'
import { useProviders } from '../../queries/useProvidersQueries'
import { isTauri } from '../../services'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { attentionColor, HEALTH_META } from '../../theme/status'
import type { HealthLevel } from '../../types'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { providerIssues } from '../../utils/providerAttention'
import { openNotificationDetails, SYSTEM_NOTIFICATION_CLICKED_EVENT } from '../../utils/systemNotifications'

import { issueIcon, issueSummary, notificationIcon, notificationLabel } from './labels'
import { NotificationDetailsModal } from './NotificationDetailsModal'

interface RowProps {
  icon: Icon
  title: string
  subtitle: string
  unread?: boolean
  onClick: () => void
}

function Row({ icon: RowIcon, title, subtitle, unread = false, onClick }: RowProps) {
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
          <Text size="sm" fw={unread ? 600 : 500} truncate>
            {title}
          </Text>
          <Text size="xs" c="dimmed" lineClamp={2}>
            {subtitle}
          </Text>
        </Stack>
        {unread && (
          <Box w={6} h={6} mt={7} bg="blue" style={{ borderRadius: '50%', flexShrink: 0 }}>
            <VisuallyHidden>Unread</VisuallyHidden>
          </Box>
        )}
      </Group>
    </UnstyledButton>
  )
}

function SectionLabel({ children }: { children: string }) {
  return (
    <Text size="xs" c="dimmed" fw={500} px="sm" pt="xs">
      {children}
    </Text>
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

  const issues = useMemo(() => providerIssues(providers ?? []), [providers])
  const unread = items.filter((item) => !item.read).length
  const lead = issues[0]
  const dotColor = lead ? attentionColor(lead.provider, lead.tone) : unread > 0 ? 'blue' : null

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

  const select = (selection: Parameters<typeof open>[0]) => {
    toggle(false)
    open(selection)
  }

  const label =
    issues.length > 0
      ? `Notifications, ${issues.length} ${issues.length === 1 ? 'provider needs' : 'providers need'} attention`
      : unread > 0
        ? `Notifications, ${unread} unread`
        : 'Notifications'

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
              {unread > 0 && (
                <Button size="compact-xs" variant="subtle" color="gray" onClick={markAllRead}>
                  Mark all read
                </Button>
              )}
            </Group>

            {issues.length === 0 && items.length === 0 ? (
              <Stack align="center" gap={4} px="md" pb="lg" pt="xs">
                <Box c="dimmed">
                  <IconBellCheck size={24} aria-hidden />
                </Box>
                <Text size="sm" fw={500}>
                  You're all caught up
                </Text>
                <Text size="xs" c="dimmed" ta="center">
                  Provider problems and pipeline failures show up here.
                </Text>
              </Stack>
            ) : (
              <ScrollArea.Autosize mah={440} type="auto">
                <Stack gap={2} px={6} pb={6}>
                  {issues.length > 0 && <SectionLabel>Provider status</SectionLabel>}
                  {issues.map((issue) => (
                    <Row
                      key={issue.provider.id}
                      icon={issueIcon(issue)}
                      title={issue.provider.name}
                      subtitle={issueSummary(issue)}
                      onClick={() => select({ type: 'provider', providerId: issue.provider.id })}
                    />
                  ))}

                  {items.length > 0 && (
                    <Group justify="space-between" pr={4}>
                      <SectionLabel>Recent</SectionLabel>
                      <Button size="compact-xs" variant="subtle" color="gray" mt="xs" onClick={clear}>
                        Clear
                      </Button>
                    </Group>
                  )}
                  {items.map((item) => (
                    <Row
                      key={item.id}
                      icon={notificationIcon(item)}
                      title={item.title}
                      subtitle={`${notificationLabel(item)} · ${formatRelativeTime(item.at, now)}`}
                      unread={!item.read}
                      onClick={() => select({ type: 'notification', id: item.id })}
                    />
                  ))}
                </Stack>
              </ScrollArea.Autosize>
            )}
          </Popover.Dropdown>
        </Popover>
      </Indicator>
      <NotificationDetailsModal />
    </>
  )
}
