import { Button, Divider, Group, Modal, Stack, Text, ThemeIcon } from '@mantine/core'
import { IconExternalLink, IconHistory } from '@tabler/icons-react'
import { useMemo } from 'react'
import { useNavigate } from 'react-router-dom'

import { useNow } from '../../hooks/useNow'
import { useProviders } from '../../queries/useProvidersQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { providerIssues } from '../../utils/providerAttention'

import { issueIcon, issueLabel, notificationIcon, notificationLabel } from './labels'
import { openLink, ProviderStatusDetails } from './ProviderStatusDetails'

const LINK_LABELS = { incident: 'View incident', status_page: 'Open status page' } as const

export function NotificationDetailsModal() {
  const selection = useNotificationCenterStore((state) => state.selection)
  const items = useNotificationCenterStore((state) => state.items)
  const close = useNotificationCenterStore((state) => state.close)
  const { data: providers } = useProviders()
  const navigate = useNavigate()
  const now = useNow(30_000)

  const issues = useMemo(() => providerIssues(providers ?? []), [providers])
  const notification =
    selection?.type === 'notification' ? (items.find((item) => item.id === selection.id) ?? null) : null
  const issue = selection?.type === 'provider' ? issues.find((item) => item.provider.id === selection.providerId) : null
  const related = notification ? issues.filter((item) => notification.providerIds.includes(item.provider.id)) : []

  const HeaderIcon = issue ? issueIcon(issue) : notification ? notificationIcon(notification) : null
  const title = issue ? issue.provider.name : (notification?.title ?? '')
  const meta = issue
    ? issue.headline === issueLabel(issue)
      ? issue.headline
      : `${issueLabel(issue)} · ${issue.headline}`
    : notification
      ? `${notificationLabel(notification)} · ${formatRelativeTime(notification.at, now)}`
      : ''

  return (
    <Modal
      opened={Boolean(issue || notification)}
      onClose={close}
      size="lg"
      title={
        <Group gap="sm" wrap="nowrap">
          {HeaderIcon && (
            <ThemeIcon variant="default" size="md" aria-hidden>
              <HeaderIcon size={16} />
            </ThemeIcon>
          )}
          <Stack gap={0}>
            <Text fw={600}>{title}</Text>
            <Text size="xs" c="dimmed">
              {meta}
            </Text>
          </Stack>
        </Group>
      }
    >
      {issue && <ProviderStatusDetails issue={issue} now={now} onNavigate={close} />}

      {notification && (
        <Stack gap="md">
          {notification.body && related.length === 0 && (
            <Text size="sm" style={{ overflowWrap: 'anywhere' }}>
              {notification.body}
            </Text>
          )}

          {(notification.link || notification.pipelineId) && (
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
            </Group>
          )}

          {related.map((item) => (
            <Stack key={item.provider.id} gap="xs">
              <Divider label={`Current status · ${item.provider.name} · ${issueLabel(item)}`} labelPosition="left" />
              <ProviderStatusDetails issue={item} now={now} onNavigate={close} />
            </Stack>
          ))}
        </Stack>
      )}
    </Modal>
  )
}
