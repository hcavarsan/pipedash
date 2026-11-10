import {
  ActionIcon,
  Button,
  Group,
  Indicator,
  Popover,
  ScrollArea,
  Stack,
  Text,
  Tooltip,
  VisuallyHidden,
} from '@mantine/core'
import {
  type Icon,
  IconAlertTriangle,
  IconBell,
  IconBellCheck,
  IconCheck,
  IconExclamationCircle,
} from '@tabler/icons-react'
import { lazy, type ReactNode, Suspense, useEffect, useRef, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useNow } from '../../../hooks/useNow'
import { useProviders } from '../../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import { HEALTH_META } from '../../../theme/status'
import type { HealthLevel } from '../../../types/health'
import type { CenterNotification, CenterNotificationTone } from '../../../types/notifications'
import type { ProviderSummary } from '../../../types/providers'
import { useNotificationActions, useNotificationItems } from '../queries/useDesktopNotifications'
import { AffectedProviderRow, NotificationCenterItem, NotificationSectionLabel } from './NotificationCenterItem'

const NotificationDetailsModal = lazy(() =>
  import('./NotificationDetailsModal').then((m) => ({ default: m.NotificationDetailsModal }))
)
const ProviderStatusModal = lazy(() =>
  import('./ProviderStatusModal').then((m) => ({ default: m.ProviderStatusModal }))
)

const DOT_COLORS: Array<[CenterNotificationTone, string]> = [
  ['danger', 'red'],
  ['warning', 'orange'],
  ['success', 'green'],
]

function unreadDotColor(items: CenterNotification[]): string | null {
  const unread = items.filter((item) => !item.read)

  return DOT_COLORS.find(([tone]) => unread.some((item) => item.tone === tone))?.[1] ?? null
}

const SEVERITIES: Record<string, { name: string; Icon: Icon }> = {
  red: { name: 'danger', Icon: IconExclamationCircle },
  orange: { name: 'warning', Icon: IconAlertTriangle },
}

function severityFor(dotColor: string | null): { name: string | null; Icon: Icon } {
  if (!dotColor) {
    return { name: null, Icon: IconCheck }
  }
  return SEVERITIES[dotColor] ?? { name: 'success', Icon: IconCheck }
}

export function NotificationHost(): ReactNode {
  const selectionType = useNotificationCenterStore((state) => state.selection?.type)
  const { data: providers } = useProviders()
  const [announcement, setAnnouncement] = useState('')
  const previousLevels = useRef<Map<number, HealthLevel> | null>(null)

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
      {selectionType === 'notification' && (
        <ErrorBoundary scope="feature" name="Notification details">
          <Suspense fallback={<LoadingState message="Loading notification details…" />}>
            <NotificationDetailsModal />
          </Suspense>
        </ErrorBoundary>
      )}
      {selectionType === 'provider' && (
        <ErrorBoundary scope="feature" name="Provider status">
          <Suspense fallback={<LoadingState message="Loading provider status…" />}>
            <ProviderStatusModal />
          </Suspense>
        </ErrorBoundary>
      )}
    </>
  )
}

interface NotificationListProps {
  title?: string
  onSelect?: () => void
}

export function useAffectedProviders(): Array<{ provider: ProviderSummary; headline: string }> {
  const { data: providers } = useProviders()

  return (providers ?? []).flatMap((provider) => {
    const { headline } = provider.attention

    return headline ? [{ provider, headline }] : []
  })
}

export function NotificationsEmptyState(): ReactNode {
  return (
    <EmptyState
      icon={<IconBellCheck size={24} aria-hidden />}
      title="You're all caught up"
      description="Provider outages and pipeline failures show up here."
    />
  )
}

export function NotificationList({ title, onSelect }: NotificationListProps): ReactNode {
  const { items, status, error, refetch } = useNotificationItems()
  const { open, markAllRead, clear, pending } = useNotificationActions()
  const now = useNow(30_000)
  const unread = items.filter((item) => !item.read).length
  const affected = useAffectedProviders()
  const empty = items.length === 0 && affected.length === 0

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
        <Group gap="3xs">
          {unread > 0 && (
            <Button
              size="compact-xs"
              variant="subtle"
              color="gray"
              loading={pending === 'markAll'}
              disabled={pending !== null}
              onClick={markAllRead}
            >
              Mark all read
            </Button>
          )}
          {items.length > 0 && (
            <Button
              size="compact-xs"
              variant="subtle"
              color="gray"
              loading={pending === 'clear'}
              disabled={pending !== null}
              onClick={clear}
            >
              Clear
            </Button>
          )}
        </Group>
      </Group>

      {status === 'error' && (
        <Stack px="md" pb="sm">
          <ErrorState title="Couldn't load notifications" error={error} onRetry={refetch} />
        </Stack>
      )}
      {status === 'pending' && empty && <LoadingState message="Loading notifications…" />}
      {status === 'success' && empty && <NotificationsEmptyState />}
      {!empty && (
        <ScrollArea.Autosize mah={title ? 440 : undefined} type="auto">
          <Stack gap="4xs" px="2xs" pb="2xs">
            {affected.length > 0 && <NotificationSectionLabel>Affecting pipelines now</NotificationSectionLabel>}
            {affected.map(({ provider, headline }) => (
              <AffectedProviderRow key={provider.id} provider={provider} headline={headline} onSelect={select} />
            ))}
            {affected.length > 0 && items.length > 0 && <NotificationSectionLabel>Recent</NotificationSectionLabel>}
            {items.map((item) => (
              <NotificationCenterItem key={item.key} item={item} now={now} onSelect={select} />
            ))}
          </Stack>
        </ScrollArea.Autosize>
      )}
    </>
  )
}

interface UnreadNotifications {
  unread: number
  dotColor: string | null
}

export function useUnreadNotifications(): UnreadNotifications {
  const { items } = useNotificationItems()

  return { unread: items.filter((item) => !item.read).length, dotColor: unreadDotColor(items) }
}

export function NotificationCenter(): ReactNode {
  const [opened, setOpened] = useState(false)
  const { unread, dotColor } = useUnreadNotifications()

  const { name: severity, Icon: SeverityIcon } = severityFor(dotColor)
  const label = unread > 0 ? `Notifications, ${unread} unread${severity ? `, ${severity}` : ''}` : 'Notifications'

  return (
    <Indicator
      color={dotColor ?? 'gray'}
      disabled={!dotColor}
      size={16}
      offset={4}
      withBorder
      label={<SeverityIcon size={14} aria-hidden />}
    >
      <Popover opened={opened} onChange={setOpened} position="bottom-end" width={380} shadow="md" trapFocus returnFocus>
        <Popover.Target>
          <Tooltip label={label} position="bottom" disabled={opened}>
            <ActionIcon size="lg" aria-label={label} onClick={() => setOpened(!opened)}>
              <IconBell size={20} aria-hidden />
            </ActionIcon>
          </Tooltip>
        </Popover.Target>

        <Popover.Dropdown p={0} maw="calc(100vw - 16px)">
          <NotificationList title="Notifications" onSelect={() => setOpened(false)} />
        </Popover.Dropdown>
      </Popover>
    </Indicator>
  )
}
