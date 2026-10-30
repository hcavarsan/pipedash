import { notifications as mantine, type NotificationData } from '@mantine/notifications'

import { PRESENT_EASING } from './motion'

// Mantine sets the slide-in transform inline; a notification's own style is the only layer merged after it.
const DESKTOP_PRESENT = { transform: 'translateX(0)', transitionTimingFunction: PRESENT_EASING }

const present = (data: NotificationData): NotificationData =>
  document.documentElement.hasAttribute('data-ios') ? data : { ...data, style: DESKTOP_PRESENT }

export const notifications = {
  ...mantine,
  show: (data: NotificationData): string => mantine.show(present(data)),
  update: (data: NotificationData): string | undefined => mantine.update(present(data)),
}
