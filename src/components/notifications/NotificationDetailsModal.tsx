import { Button, Group, Modal, Stack, Text, ThemeIcon } from '@mantine/core'
import { IconExternalLink, IconHeartbeat, IconHistory } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { useProviders } from '../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'

import { notificationIcon, notificationLabel, TONE_COLORS } from './labels'
import { openNotificationLink } from './openNotificationLink'

const LINK_LABELS = { incident: 'View incident', status_page: 'Open status page' } as const

export function NotificationDetailsModal(): ReactNode {
  const selection = useNotificationCenterStore((state) => state.selection)
  const items = useNotificationCenterStore((state) => state.items)
  const open = useNotificationCenterStore((state) => state.open)
  const close = useNotificationCenterStore((state) => state.close)
  const { data: providers } = useProviders()
  const navigate = useNavigate()
  const location = useLocation()
  const openHistory = (pipelineId: string) => {
    close()
    void navigate(`/pipelines/${encodeURIComponent(pipelineId)}`, {
      state: { from: location.pathname + location.search },
    })
  }
  const now = useNow(30_000)
  const mobileShell = useMobileShell()

  const notification =
    selection?.type === 'notification' ? (items.find((item) => item.key === selection.key) ?? null) : null
  const related = notification
    ? (providers ?? []).filter((provider) => notification.providerIds.includes(provider.id))
    : []
  const HeaderIcon = notification ? notificationIcon(notification) : null

  if (mobileShell) {
    return (
      <Modal opened={Boolean(notification)} onClose={close} title={notification?.title}>
        {notification && HeaderIcon && (
          <Stack gap={24}>
            <IosGroup>
              <IosRow
                icon={<HeaderIcon size={18} color={TONE_COLORS[notification.tone]} aria-hidden />}
                title={notificationLabel(notification)}
                value={formatRelativeTime(notification.at, now)}
              />
            </IosGroup>

            {notification.body && (
              <IosGroup header="Details">
                <IosRow title={notification.body} />
              </IosGroup>
            )}

            {(notification.link || notification.pipelineId || related.length > 0) && (
              <IosGroup>
                {notification.link && (
                  <IosRow
                    icon={<IconExternalLink size={18} aria-hidden />}
                    title={LINK_LABELS[notification.link.kind]}
                    chevron
                    onClick={() => notification.link && openNotificationLink(notification.link.url)}
                  />
                )}
                {notification.pipelineId && (
                  <IosRow
                    icon={<IconHistory size={18} aria-hidden />}
                    title="Open run history"
                    chevron
                    onClick={() => openHistory(notification.pipelineId ?? '')}
                  />
                )}
                {related.map((provider) => (
                  <IosRow
                    key={provider.id}
                    icon={<IconHeartbeat size={18} aria-hidden />}
                    title={related.length > 1 ? `${provider.name} status` : 'View service status'}
                    chevron
                    onClick={() => open({ type: 'provider', providerId: provider.id })}
                  />
                ))}
              </IosGroup>
            )}
          </Stack>
        )}
      </Modal>
    )
  }

  return (
    <StandardModal
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
                onClick={() => notification.link && openNotificationLink(notification.link.url)}
              >
                {LINK_LABELS[notification.link.kind]}
              </Button>
            )}
            {notification.pipelineId && (
              <Button
                size="compact-sm"
                variant="default"
                leftSection={<IconHistory size={14} aria-hidden />}
                onClick={() => openHistory(notification.pipelineId ?? '')}
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
    </StandardModal>
  )
}
