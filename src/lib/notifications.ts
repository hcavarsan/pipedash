import { notifications as mantine, type NotificationData } from '@mantine/notifications'

import { PRESENT_EASING } from './motion'

const DESKTOP_PRESENT = { transform: 'translateX(0)', transitionTimingFunction: PRESENT_EASING }

const present = (data: NotificationData): NotificationData =>
  document.documentElement.hasAttribute('data-ios') ? data : { ...data, style: DESKTOP_PRESENT }

export const notifications = {
  ...mantine,
  show: (data: NotificationData): string => mantine.show(present(data)),
  update: (data: NotificationData): string | undefined => mantine.update(present(data)),
}
