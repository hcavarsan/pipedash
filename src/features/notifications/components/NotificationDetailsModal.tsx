import { Button, Group, Stack, Text, ThemeIcon } from '@mantine/core'
import type { ReactNode } from 'react'
import { StandardModal } from '../../../components/common/StandardModal'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import type { CenterNotification } from '../../../types/notifications'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { type NotificationDetailAction, useNotificationDetailActions } from '../hooks/useNotificationDetailActions'
import { notificationIcon, notificationLabel, TONE_COLORS } from '../lib/labels'
import { useNotificationItems } from '../queries/useDesktopNotifications'

interface DetailsViewProps {
  notification: CenterNotification
  actions: NotificationDetailAction[]
  now: number
}

function MobileDetails({ notification, actions, now }: DetailsViewProps): ReactNode {
  const HeaderIcon = notificationIcon(notification)

  return (
    <Stack gap="lg">
      <IosGroup>
        <IosRow
          icon={<HeaderIcon size={16} color={TONE_COLORS[notification.tone]} aria-hidden />}
          title={notificationLabel(notification)}
          value={formatRelativeTime(notification.at, now)}
        />
      </IosGroup>

      {notification.body && (
        <IosGroup header="Details">
          <IosRow title={notification.body} />
        </IosGroup>
      )}

      {actions.length > 0 && (
        <IosGroup>
          {actions.map((action) => (
            <IosRow
              key={action.key}
              icon={action.icon(16)}
              title={action.label}
              chevron={action.chevron}
              onClick={action.onClick}
            />
          ))}
        </IosGroup>
      )}
    </Stack>
  )
}

function DesktopTitle({ notification, now }: Pick<DetailsViewProps, 'notification' | 'now'>): ReactNode {
  const HeaderIcon = notificationIcon(notification)

  return (
    <Group gap="sm" wrap="nowrap">
      <ThemeIcon variant="default" size="md" aria-hidden>
        <HeaderIcon size={16} color={TONE_COLORS[notification.tone]} />
      </ThemeIcon>
      <Stack gap={0}>
        <Text fw={600}>{notification.title}</Text>
        <Text size="xs" c="dimmed">
          {notificationLabel(notification)} · {formatRelativeTime(notification.at, now)}
        </Text>
      </Stack>
    </Group>
  )
}

function DesktopDetails({ notification, actions }: Pick<DetailsViewProps, 'notification' | 'actions'>): ReactNode {
  return (
    <Stack gap="md">
      {notification.body && (
        <Text size="sm" style={{ overflowWrap: 'anywhere' }}>
          {notification.body}
        </Text>
      )}

      <Group gap="xs">
        {actions.map((action) => (
          <Button
            key={action.key}
            size="compact-sm"
            variant="default"
            leftSection={action.icon(14)}
            onClick={action.onClick}
          >
            {action.label}
          </Button>
        ))}
      </Group>
    </Stack>
  )
}

export function NotificationDetailsModal(): ReactNode {
  const selection = useNotificationCenterStore((state) => state.selection)
  const close = useNotificationCenterStore((state) => state.close)
  const { items } = useNotificationItems()
  const now = useNow(30_000)
  const mobileShell = useMobileShell()

  const notification =
    selection?.type === 'notification' ? (items.find((item) => item.key === selection.key) ?? null) : null
  const actions = useNotificationDetailActions(notification)
  let title: ReactNode = null
  let body: ReactNode = null

  if (notification) {
    title = mobileShell ? notification.title : <DesktopTitle notification={notification} now={now} />
    body = mobileShell ? (
      <MobileDetails notification={notification} actions={actions} now={now} />
    ) : (
      <DesktopDetails notification={notification} actions={actions} />
    )
  }

  return (
    <StandardModal opened={Boolean(notification)} onClose={close} size="md" title={title}>
      {body}
    </StandardModal>
  )
}
