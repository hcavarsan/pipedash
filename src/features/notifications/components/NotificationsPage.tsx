import { Loader, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { PageContainer } from '../../../components/common/PageContainer'
import { PageHeader } from '../../../components/common/PageHeader'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import type { CenterNotification } from '../../../types/notifications'
import { openConfirm } from '../../../utils/confirm'

import { useNotificationActions, useNotificationItems } from '../queries/useDesktopNotifications'

import { NotificationList, NotificationsEmptyState, useAffectedProviders } from './NotificationCenter'
import { NotificationListItem } from './NotificationListItem'

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
  const { items, status, error, refetch } = useNotificationItems()
  const { open, markAllRead, markRead, clear, pending } = useNotificationActions()
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

  const markAllReadTitle = (
    <Text span inherit c="var(--mantine-color-blue-light-color)">
      Mark all as read
    </Text>
  )
  const failure = status === 'error' && (
    <ErrorState title="Couldn't load notifications" error={error} onRetry={refetch} />
  )

  if (items.length === 0 && affected.length === 0) {
    if (status === 'pending') {
      return <LoadingState message="Loading notifications…" />
    }

    return failure || <NotificationsEmptyState />
  }

  return (
    <Stack gap="lg">
      {failure}
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
          {group.items.map((item) => (
            <NotificationListItem key={item.key} item={item} now={now} onOpen={open} onMarkRead={markRead} />
          ))}
        </IosGroup>
      ))}

      {unread && (
        <IosGroup>
          <IosRow
            title={markAllReadTitle}
            disabled={pending !== null}
            accessory={pending === 'markAll' ? <Loader size="xs" aria-label="Marking as read" /> : undefined}
            onClick={markAllRead}
          />
        </IosGroup>
      )}

      {items.length > 0 && (
        <IosGroup>
          <IosRow
            destructive
            title="Clear all"
            disabled={pending !== null}
            accessory={pending === 'clear' ? <Loader size="xs" aria-label="Clearing" /> : undefined}
            onClick={confirmClear}
          />
        </IosGroup>
      )}
    </Stack>
  )
}

export function NotificationsPage(): ReactNode {
  const mobileShell = useMobileShell()

  return mobileShell ? (
    <ShellNotificationList />
  ) : (
    <PageContainer>
      <PageHeader title="Notifications" />
      <NotificationList />
    </PageContainer>
  )
}
