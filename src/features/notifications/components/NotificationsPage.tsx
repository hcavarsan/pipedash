import { Box, Stack, Text, VisuallyHidden } from '@mantine/core'
import { IconBellCheck } from '@tabler/icons-react'
import { type ReactNode, useEffect } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { PageContainer } from '../../../components/common/PageContainer'
import { PageHeader } from '../../../components/common/PageHeader'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import type { CenterNotification } from '../../../stores/notificationCenterStore'
import { openConfirm } from '../../../utils/confirm'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'

import { notificationIcon, notificationLabel, TONE_COLORS } from '../lib/labels'
import { useNotificationActions, useNotificationItems } from '../queries/useDesktopNotifications'

import { NotificationList, useAffectedProviders } from './NotificationCenter'

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
  const items = useNotificationItems()
  const { open, markAllRead, markRead, clear } = useNotificationActions()
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
                {...(item.read
                  ? {}
                  : {
                      swipeActions: [
                        {
                          key: 'read',
                          label: 'Mark read',
                          tone: 'blue' as const,
                          primary: true,
                          onSelect: () => markRead(item.key),
                        },
                      ],
                    })}
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

export function NotificationsPage(): ReactNode {
  const { markAllRead } = useNotificationActions()
  const mobileShell = useMobileShell()

  useEffect(() => markAllRead, [markAllRead])

  return mobileShell ? (
    <ShellNotificationList />
  ) : (
    <PageContainer>
      <PageHeader title="Notifications" />
      <NotificationList />
    </PageContainer>
  )
}
