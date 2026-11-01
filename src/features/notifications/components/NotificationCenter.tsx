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
import { IconAlertTriangle, IconBell, IconBellCheck, IconCheck, IconExclamationCircle } from '@tabler/icons-react'
import { lazy, type ReactNode, Suspense, useEffect, useRef, useState } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { EmptyState } from '../../../components/common/EmptyState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useNow } from '../../../hooks/useNow'
import { useProviders } from '../../../queries/useProvidersQueries'
import {
  type CenterNotification,
  type CenterNotificationTone,
  useNotificationCenterStore,
} from '../../../stores/notificationCenterStore'
import { HEALTH_META } from '../../../theme/status'
import type { HealthLevel, ProviderSummary } from '../../../types/domain'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { providerAttention } from '../../../utils/providerAttention'

import { notificationIcon, notificationLabel } from '../lib/labels'
import { useNotificationActions, useNotificationItems } from '../queries/useDesktopNotifications'

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
      py="xs"
      onClick={onClick}
      style={{ borderRadius: 'var(--mantine-radius-sm)' }}
    >
      <Group gap="sm" wrap="nowrap" align="flex-start">
        <Box c="dimmed" pt="4xs" style={{ display: 'flex' }}>
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
          <Box w={6} h={6} mt="2xs" bg="blue" style={{ borderRadius: '50%', flexShrink: 0 }}>
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
          <Suspense fallback={<LoadingState message="Loading dialog..." />}>
            <NotificationDetailsModal />
          </Suspense>
        </ErrorBoundary>
      )}
      {selectionType === 'provider' && (
        <ErrorBoundary scope="feature" name="Provider status">
          <Suspense fallback={<LoadingState message="Loading dialog..." />}>
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
    const { headline } = providerAttention(provider)

    return headline ? [{ provider, headline }] : []
  })
}

export function NotificationList({ title, onSelect }: NotificationListProps): ReactNode {
  const items = useNotificationItems()
  const { open, markAllRead, clear } = useNotificationActions()
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
        <Group gap="3xs">
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
        <EmptyState
          icon={<IconBellCheck size={24} aria-hidden />}
          title="You're all caught up"
          description="Provider outages and pipeline failures show up here."
        />
      ) : (
        <ScrollArea.Autosize mah={title ? 440 : undefined} type="auto">
          <Stack gap="4xs" px="2xs" pb="2xs">
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

interface UnreadNotifications {
  unread: number
  dotColor: string | null
}

export function useUnreadNotifications(): UnreadNotifications {
  const items = useNotificationItems()

  return { unread: items.filter((item) => !item.read).length, dotColor: unreadDotColor(items) }
}

export function NotificationCenter(): ReactNode {
  const [opened, setOpened] = useState(false)
  const { markAllRead } = useNotificationActions()
  const { unread, dotColor } = useUnreadNotifications()

  const toggle = (next: boolean) => {
    if (!next) {
      markAllRead()
    }
    setOpened(next)
  }

  const severity = dotColor === 'red' ? 'danger' : dotColor === 'orange' ? 'warning' : dotColor ? 'success' : null
  const SeverityIcon =
    severity === 'danger' ? IconExclamationCircle : severity === 'warning' ? IconAlertTriangle : IconCheck
  const label = unread > 0 ? `Notifications, ${unread} unread${severity ? `, ${severity}` : ''}` : 'Notifications'

  return (
    <Indicator
      color={dotColor ?? 'gray'}
      disabled={!dotColor}
      size={16}
      offset={4}
      withBorder
      label={<SeverityIcon size={12} aria-hidden />}
    >
      <Popover opened={opened} onChange={toggle} position="bottom-end" width={380} shadow="md" trapFocus returnFocus>
        <Popover.Target>
          <Tooltip label={label} position="bottom" disabled={opened}>
            <ActionIcon size="lg" aria-label={label} onClick={() => toggle(!opened)}>
              <IconBell size={20} aria-hidden />
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
