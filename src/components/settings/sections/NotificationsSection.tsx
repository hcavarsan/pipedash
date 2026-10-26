import { Card, Stack, Switch } from '@mantine/core'
import { useState } from 'react'

import { isMobileShell, useMobileShell } from '@/hooks/useIsMobile'
import { useLocalNotificationSettings, useSetLocalNotifications } from '@/queries/useLocalNotificationQueries'
import { isTauri } from '@/services'
import { useNotificationStore } from '@/stores/notificationStore'
import { openConfirm } from '@/utils/confirm'
import { displayWarningNotification } from '@/utils/errorDisplay'
import { isIosPlatform, isMobilePlatform } from '@/utils/platform'
import {
  notificationProblem,
  requestSystemNotificationPermission,
  systemNotificationsSupported,
} from '@/utils/systemNotifications'

import { IosGroup } from '../../ios/IosList'
import { MobileDevicesCard } from '../MobileDevicesCard'
import { SwitchRow } from '../mobileParts'
import { NotificationsBlockedRows } from '../NotificationsBlockedRows'
import { PushNotificationsCard } from '../PushNotificationsCard'

const DESCRIPTION =
  'Get a system notification when a pipeline fails or runs longer than usual, and when a provider has an outage or recovers.'
const IOS_DESCRIPTION = `${DESCRIPTION} Pipedash also checks in the background when iOS allows it.`

function confirmBackgroundAccess(): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>()

  openConfirm({
    title: 'Turn on notifications?',
    children:
      'While this device is locked, Pipedash uses the provider credentials saved on it to check for failed pipelines, long-running runs, and provider outages. iOS decides when these checks happen, so alerts can be delayed. iOS asks to allow notifications next. Turning notifications off removes this access.',
    labels: isMobileShell() ? { confirm: 'Turn On', cancel: 'Cancel' } : { confirm: 'Turn on', cancel: 'Cancel' },
    onConfirm: () => resolve(true),
    onCancel: () => resolve(false),
    onClose: () => resolve(false),
  })

  return promise
}

export function NotificationsSection() {
  const mobileShell = useMobileShell()
  const ios = isIosPlatform()
  const storeEnabled = useNotificationStore((s) => s.notificationsEnabled)
  const setStoreEnabled = useNotificationStore((s) => s.setNotificationsEnabled)
  const settings = useLocalNotificationSettings()
  const setLocalEnabled = useSetLocalNotifications()
  const notificationsSupported = systemNotificationsSupported()
  const [blocked, setBlocked] = useState(() => 'Notification' in window && Notification.permission === 'denied')
  const [busy, setBusy] = useState(false)

  const enabled = notificationsSupported && (ios ? settings.data?.enabled === true : storeEnabled)
  const disabled = !notificationsSupported || busy || (ios && settings.data === undefined)
  const description = ios ? IOS_DESCRIPTION : DESCRIPTION
  let note: string | undefined
  if (!notificationsSupported) {
    note = notificationProblem('unsupported')
  } else if (ios && settings.isError) {
    note = "Couldn't read the notification settings for this device."
  } else if (ios && settings.data?.enabled && !settings.data.available) {
    note = settings.data.reason ?? undefined
  }

  const handleToggle = async (next: boolean) => {
    if (busy) {
      return
    }

    setBusy(true)
    try {
      if (next) {
        if (ios && !(await confirmBackgroundAccess())) {
          return
        }

        if (!(await requestSystemNotificationPermission())) {
          setBlocked(true)
          displayWarningNotification(notificationProblem('blocked'), 'Notifications blocked')

          return
        }

        setBlocked(false)
      }

      if (!ios) {
        setStoreEnabled(next)

        return
      }

      const result = await setLocalEnabled.mutateAsync(next).catch(() => null)

      if (next && result && !result.enabled) {
        displayWarningNotification(result.reason ?? "Notifications couldn't be turned on.", 'Notifications are off')
      }
    } finally {
      setBusy(false)
    }
  }

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <IosGroup footer={note}>
          <SwitchRow
            title="Notifications"
            subtitle={description}
            checked={enabled}
            onChange={(e) => handleToggle(e.currentTarget.checked)}
            disabled={disabled}
          />
        </IosGroup>

        {blocked && notificationsSupported && (
          <IosGroup>
            <NotificationsBlockedRows />
          </IosGroup>
        )}

        {isMobilePlatform() && <PushNotificationsCard />}
        {!isTauri() && <MobileDevicesCard />}
      </Stack>
    )
  }

  return (
    <Stack gap="md">
      <Card padding="md">
        <Switch
          label="Notifications"
          description={note ?? description}
          checked={enabled}
          onChange={(e) => handleToggle(e.currentTarget.checked)}
          disabled={disabled}
        />
      </Card>

      {isMobilePlatform() && <PushNotificationsCard />}
      {!isTauri() && <MobileDevicesCard />}
    </Stack>
  )
}
