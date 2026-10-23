import { Button, Group, Modal, Stack, Text, ThemeIcon } from '@mantine/core'
import { IconExternalLink, IconHeartbeat, IconHistory } from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'

import { useNow } from '../../hooks/useNow'
import { useProviders } from '../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { formatRelativeTime } from '../../utils/formatRelativeTime'

import { notificationIcon, notificationLabel } from './labels'
import { openLink } from './ProviderStatusModal'

const LINK_LABELS = { incident: 'View incident', status_page: 'Open status page' } as const

export function NotificationDetailsModal() {
  const selection = useNotificationCenterStore((state) => state.selection)
  const items = useNotificationCenterStore((state) => state.items)
  const open = useNotificationCenterStore((state) => state.open)
  const close = useNotificationCenterStore((state) => state.close)
  const { data: providers } = useProviders()
  const navigate = useNavigate()
  const now = useNow(30_000)

  const notification =
    selection?.type === 'notification' ? (items.find((item) => item.key === selection.key) ?? null) : null
  const related = notification
    ? (providers ?? []).filter((provider) => notification.providerIds.includes(provider.id))
    : []
  const HeaderIcon = notification ? notificationIcon(notification) : null

  return (
    <Modal
      opened={Boolean(notification)}
      onClose={close}
      size="md"
      title={
        notification && (
          <Group gap="sm" wrap="nowrap">
            {HeaderIcon && (
              <ThemeIcon variant="default" size="md" aria-hidden>
                <HeaderIcon size={16} />
              </ThemeIcon>
            )}
            <Stack gap={0}>
              <Text fw={600}>{notification.title}</Text>
              <Text size="xs" c="dimmed">
                {notificationLabel(notification)} · {formatRelativeTime(notification.at, now)}
              </Text>
            </Stack>
          </Group>
        )
      }
    >
      {notification && (
        <Stack gap="md">
          {notification.body && (
            <Text size="sm" style={{ overflowWrap: 'anywhere' }}>
              {notification.body}
            </Text>
          )}

          <Group gap="xs">
            {notification.link && (
              <Button
                size="compact-sm"
                variant="default"
                leftSection={<IconExternalLink size={14} aria-hidden />}
                onClick={() => notification.link && openLink(notification.link.url)}
              >
                {LINK_LABELS[notification.link.kind]}
              </Button>
            )}
            {notification.pipelineId && (
              <Button
                size="compact-sm"
                variant="default"
                leftSection={<IconHistory size={14} aria-hidden />}
                onClick={() => {
                  close()
                  navigate(`/pipelines/${encodeURIComponent(notification.pipelineId ?? '')}`)
                }}
              >
                Open run history
              </Button>
            )}
            {related.map((provider) => (
              <Button
                key={provider.id}
                size="compact-sm"
                variant="default"
                leftSection={<IconHeartbeat size={14} aria-hidden />}
                onClick={() => open({ type: 'provider', providerId: provider.id })}
              >
                {related.length > 1 ? `${provider.name} status` : 'View service status'}
              </Button>
            ))}
          </Group>
        </Stack>
      )}
    </Modal>
  )
}
