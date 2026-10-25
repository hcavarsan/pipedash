import { Notifications } from '@mantine/notifications'

import { useBannerSwipe } from '../hooks/useBannerSwipe'
import { useMobileShell } from '../hooks/useIsMobile'
import { iosToastClassNames } from './ios'

export function ToastHost() {
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

  return <Notifications position="bottom-right" pauseResetOnHover="notification" />
}
