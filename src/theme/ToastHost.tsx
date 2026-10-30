import { Notifications } from '@mantine/notifications'
import type { ReactNode } from 'react'

import { useBannerSwipe } from '../hooks/useBannerSwipe'
import { useMobileShell } from '../hooks/useIsMobile'
import { iosToastClassNames } from './ios'

export function ToastHost(): ReactNode {
  const mobileShell = useMobileShell()

  useBannerSwipe(mobileShell)

  if (mobileShell) {
    return (
      <Notifications
        position="top-center"
        limit={3}
        zIndex={600}
        classNames={iosToastClassNames}
        allowDragDismiss={false}
        allowScrollDismiss={false}
      />
    )
  }

  return (
    <Notifications
      position="bottom-right"
      pauseResetOnHover="notification"
      allowDragDismiss={false}
      allowScrollDismiss={false}
      transitionDuration={160}
    />
  )
}
