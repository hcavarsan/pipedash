import { Box, Group, Stack, Text, UnstyledButton, VisuallyHidden } from '@mantine/core'
import type { ReactNode } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import type { CenterSelection } from '../../../stores/notificationCenterStore'
import type { CenterNotification } from '../../../types/notifications'
import type { ProviderSummary } from '../../../types/providers'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { notificationIcon, notificationLabel } from '../lib/labels'

interface RowProps {
  leading: ReactNode
  title: string
  subtitle: string
  unread?: boolean
  onClick: () => void
}

function Row({ leading, title, subtitle, unread = false, onClick }: RowProps): ReactNode {
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

export function NotificationSectionLabel({ children }: { children: string }): ReactNode {
  return (
    <Text size="xs" c="dimmed" fw={500} px="sm" pt="xs">
      {children}
    </Text>
  )
}

interface AffectedProviderRowProps {
  provider: ProviderSummary
  headline: string
  onSelect: (selection: CenterSelection) => void
}

export function AffectedProviderRow({ provider, headline, onSelect }: AffectedProviderRowProps): ReactNode {
  return (
    <Row
      leading={<ProviderIcon name={provider.name} icon={provider.icon} size={16} />}
      title={provider.name}
      subtitle={headline}
      onClick={() => onSelect({ type: 'provider', providerId: provider.id })}
    />
  )
}

interface NotificationCenterItemProps {
  item: CenterNotification
  now: number
  onSelect: (selection: CenterSelection) => void
}

export function NotificationCenterItem({ item, now, onSelect }: NotificationCenterItemProps): ReactNode {
  const ItemIcon = notificationIcon(item)

  return (
    <Row
      leading={<ItemIcon size={16} aria-hidden />}
      title={item.title}
      subtitle={`${notificationLabel(item)} · ${formatRelativeTime(item.at, now)}`}
      unread={!item.read}
      onClick={() => onSelect({ type: 'notification', key: item.key })}
    />
  )
}
