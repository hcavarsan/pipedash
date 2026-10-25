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
import { type ReactNode, useEffect, useRef, useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
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
import { openConfirm } from '../../utils/confirm'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { providerAttention } from '../../utils/providerAttention'
import { openNotificationDetails, SYSTEM_NOTIFICATION_CLICKED_EVENT } from '../../utils/systemNotifications'
import { ProviderIcon } from '../atoms/ProviderIcon'
import { IosGroup, IosRow } from '../ios/IosList'

import { notificationIcon, notificationLabel, TONE_COLORS } from './labels'
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

interface RowProps {
  leading: ReactNode
  title: string
  subtitle: string
  unread?: boolean
  onClick: () => void
}

function Row({ leading, title, subtitle, unread = false, onClick }: RowProps) {
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
          {leading}
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

export function NotificationHost() {
  const { data: providers } = useProviders()
  const [announcement, setAnnouncement] = useState('')
  const previousLevels = useRef<Map<number, HealthLevel> | null>(null)

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
      .catch((error: unknown) => logger.warn('NotificationHost', "Couldn't listen for notification clicks", error))

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

  return (
    <>
      <VisuallyHidden role="status">{announcement}</VisuallyHidden>
      <NotificationDetailsModal />
      <ProviderStatusModal />
    </>
  )
}

interface NotificationListProps {
  title?: string
  onSelect?: () => void
}

function useAffectedProviders() {
  const { data: providers } = useProviders()

  return (providers ?? []).flatMap((provider) => {
    const { headline } = providerAttention(provider)

    return headline ? [{ provider, headline }] : []
  })
}

function NotificationList({ title, onSelect }: NotificationListProps) {
  const items = useNotificationCenterStore((state) => state.items)
  const open = useNotificationCenterStore((state) => state.open)
  const markAllRead = useNotificationCenterStore((state) => state.markAllRead)
  const clear = useNotificationCenterStore((state) => state.clear)
  const now = useNow(30_000)
  const unread = items.filter((item) => !item.read).length
  const affected = useAffectedProviders()

  const select = (selection: Parameters<typeof open>[0]) => {
    onSelect?.()
    open(selection)
  }

  return (
    <>
      <Group justify={title ? 'space-between' : 'flex-end'} px="md" py="sm" mih={44}>
        {title && (
          <Text fw={600} size="sm">
            {title}
          </Text>
        )}
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

      {items.length === 0 && affected.length === 0 ? (
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
        <ScrollArea.Autosize mah={title ? 440 : undefined} type="auto">
          <Stack gap={2} px={6} pb={6}>
            {affected.length > 0 && <SectionLabel>Affecting pipelines now</SectionLabel>}
            {affected.map(({ provider, headline }) => (
              <Row
                key={provider.id}
                leading={<ProviderIcon name={provider.name} icon={provider.icon} size={16} />}
                title={provider.name}
                subtitle={headline}
                onClick={() => select({ type: 'provider', providerId: provider.id })}
              />
            ))}
            {affected.length > 0 && items.length > 0 && <SectionLabel>Recent</SectionLabel>}
            {items.map((item) => {
              const ItemIcon = notificationIcon(item)

              return (
                <Row
                  key={item.key}
                  leading={<ItemIcon size={16} aria-hidden />}
                  title={item.title}
                  subtitle={`${notificationLabel(item)} · ${formatRelativeTime(item.at, now)}`}
                  unread={!item.read}
                  onClick={() => select({ type: 'notification', key: item.key })}
                />
              )
            })}
          </Stack>
        </ScrollArea.Autosize>
      )}
    </>
  )
}

const DAY_MS = 86_400_000
const DAY_FORMAT = new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'short', day: 'numeric' })

function dayLabel(at: number, now: number): string {
  const days = Math.round((new Date(now).setHours(0, 0, 0, 0) - new Date(at).setHours(0, 0, 0, 0)) / DAY_MS)

  if (days <= 0) {
    return 'Today'
  }

  return days === 1 ? 'Yesterday' : DAY_FORMAT.format(at)
}

function groupByDay(items: CenterNotification[], now: number) {
  const groups: Array<{ label: string; items: CenterNotification[] }> = []

  for (const item of items) {
    const label = dayLabel(item.at, now)
    const last = groups[groups.length - 1]

    if (last?.label === label) {
      last.items.push(item)
    } else {
      groups.push({ label, items: [item] })
    }
  }

  return groups
}

function ShellNotificationList() {
  const items = useNotificationCenterStore((state) => state.items)
  const open = useNotificationCenterStore((state) => state.open)
  const markAllRead = useNotificationCenterStore((state) => state.markAllRead)
  const clear = useNotificationCenterStore((state) => state.clear)
  const now = useNow(30_000)
  const affected = useAffectedProviders()
  const unread = items.some((item) => !item.read)

  const confirmClear = () =>
    openConfirm({
      title: 'Clear all notifications?',
      children: <Text size="sm">Every notification is removed from this list.</Text>,
      labels: { confirm: 'Clear all', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: clear,
    })

  if (items.length === 0 && affected.length === 0) {
    return (
      <Stack align="center" gap={6} px="xl" pt={72}>
        <Box c="dimmed">
          <IconBellCheck size={48} stroke={1.4} aria-hidden />
        </Box>
        <Text size="lg" fw={600}>
          You're all caught up
        </Text>
        <Text size="sm" c="dimmed" ta="center">
          Provider outages and pipeline failures show up here.
        </Text>
      </Stack>
    )
  }

  return (
    <Stack gap={24}>
      {affected.length > 0 && (
        <IosGroup header="Affecting pipelines now">
          {affected.map(({ provider, headline }) => (
            <IosRow
              key={provider.id}
              icon={<ProviderIcon name={provider.name} icon={provider.icon} size={20} />}
              title={provider.name}
              subtitle={headline}
              chevron
              onClick={() => open({ type: 'provider', providerId: provider.id })}
            />
          ))}
        </IosGroup>
      )}

      {groupByDay(items, now).map((group) => (
        <IosGroup key={group.label} header={group.label}>
          {group.items.map((item) => {
            const ItemIcon = notificationIcon(item)

            return (
              <IosRow
                key={item.key}
                icon={<ItemIcon size={18} color={TONE_COLORS[item.tone]} aria-hidden />}
                title={
                  <Text span inherit fw={item.read ? 400 : 600}>
                    {item.title}
                  </Text>
                }
                subtitle={`${notificationLabel(item)} · ${formatRelativeTime(item.at, now)}`}
                accessory={
                  item.read ? undefined : (
                    <Box w={10} h={10} bg="blue" style={{ borderRadius: '50%' }}>
                      <VisuallyHidden>Unread</VisuallyHidden>
                    </Box>
                  )
                }
                chevron
                onClick={() => open({ type: 'notification', key: item.key })}
              />
            )
          })}
        </IosGroup>
      ))}

      {unread && (
        <IosGroup>
          <IosRow
            title={
              <Text span inherit c="var(--mantine-color-blue-light-color)">
                Mark all as read
              </Text>
            }
            onClick={markAllRead}
          />
        </IosGroup>
      )}

      {items.length > 0 && (
        <IosGroup>
          <IosRow destructive title="Clear all" onClick={confirmClear} />
        </IosGroup>
      )}
    </Stack>
  )
}

export function useUnreadNotifications() {
  const items = useNotificationCenterStore((state) => state.items)

  return { unread: items.filter((item) => !item.read).length, dotColor: unreadDotColor(items) }
}

export function NotificationsPage() {
  const markAllRead = useNotificationCenterStore((state) => state.markAllRead)
  const mobileShell = useMobileShell()

  useEffect(() => markAllRead, [markAllRead])

  return mobileShell ? <ShellNotificationList /> : <NotificationList />
}

export function NotificationCenter() {
  const [opened, setOpened] = useState(false)
  const markAllRead = useNotificationCenterStore((state) => state.markAllRead)
  const { unread, dotColor } = useUnreadNotifications()

  const toggle = (next: boolean) => {
    if (!next) {
      markAllRead()
    }
    setOpened(next)
  }

  const label = unread > 0 ? `Notifications, ${unread} unread` : 'Notifications'

  return (
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
          <NotificationList title="Notifications" onSelect={() => toggle(false)} />
        </Popover.Dropdown>
      </Popover>
    </Indicator>
  )
}
