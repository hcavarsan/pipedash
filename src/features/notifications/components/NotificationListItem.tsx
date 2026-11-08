import { Box, Text, VisuallyHidden } from '@mantine/core'
import type { ReactNode } from 'react'
import { IosRow } from '../../../components/ios/IosList'
import type { CenterSelection } from '../../../stores/notificationCenterStore'
import type { CenterNotification } from '../../../types/notifications'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { notificationIcon, notificationLabel, TONE_COLORS } from '../lib/labels'

interface NotificationListItemProps {
  item: CenterNotification
  now: number
  onOpen: (selection: CenterSelection) => void
  onMarkRead: (key: string) => void
}

export function NotificationListItem({ item, now, onOpen, onMarkRead }: NotificationListItemProps): ReactNode {
  const ItemIcon = notificationIcon(item)
  const unreadProps = item.read
    ? {}
    : {
        swipeActions: [
          {
            key: 'read',
            label: 'Mark read',
            tone: 'blue' as const,
            primary: true,
            onSelect: () => onMarkRead(item.key),
          },
        ],
      }
  const unreadDot = item.read ? undefined : (
    <Box w={10} h={10} bg="blue" style={{ borderRadius: '50%' }}>
      <VisuallyHidden>Unread</VisuallyHidden>
    </Box>
  )

  return (
    <IosRow
      icon={<ItemIcon size={18} color={TONE_COLORS[item.tone]} aria-hidden />}
      title={
        <Text span inherit fw={item.read ? 400 : 600}>
          {item.title}
        </Text>
      }
      subtitle={`${notificationLabel(item)} · ${formatRelativeTime(item.at, now)}`}
      accessory={unreadDot}
      chevron
      onClick={() => onOpen({ type: 'notification', key: item.key })}
      {...unreadProps}
    />
  )
}
