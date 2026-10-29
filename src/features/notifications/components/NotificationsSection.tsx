import { Card, Stack, Switch } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import {
  useLocalNotificationSettings,
  useSetLocalNotifications,
} from '@/features/notifications/queries/useLocalNotificationQueries'
import { isMobileShell, useMobileShell } from '@/hooks/useIsMobile'
import { isTauri } from '@/services/isTauri'
import { useNotificationStore } from '@/stores/notificationStore'
import { openConfirm } from '@/utils/confirm'
import { displayWarningNotification } from '@/utils/errorDisplay'
import { isAndroidPlatform, isMobilePlatform } from '@/utils/platform'
import {
  notificationProblem,
  requestSystemNotificationPermission,
  systemNotificationsSupported,
} from '@/utils/systemNotifications'

import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { IosGroup } from '../../../components/ios/IosList'
import { SwitchRow } from '../../../components/ios/mobileParts'
import { MobileDevicesCard } from './MobileDevicesCard'
import { NotificationsBlockedRows } from './NotificationsBlockedRows'
import { PushNotificationsCard } from './PushNotificationsCard'

const DESCRIPTION =
  'Get a system notification when a pipeline fails or runs longer than usual, and when a provider has an outage or recovers.'

function confirmBackgroundAccess(system: string): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>()

  openConfirm({
    title: 'Turn on notifications?',
    children: `While this device is locked, Pipedash uses the provider credentials saved on it to check for failed pipelines, long-running runs, and provider outages. ${system} decides when these checks happen, so alerts can be delayed. ${system} asks to allow notifications next. Turning notifications off removes this access.`,
    labels: isMobileShell() ? { confirm: 'Turn On', cancel: 'Cancel' } : { confirm: 'Turn on', cancel: 'Cancel' },
    onConfirm: () => resolve(true),
    onCancel: () => resolve(false),
    onClose: () => resolve(false),
  })

  return promise
}

export function NotificationsSection(): ReactNode {
  const mobileShell = useMobileShell()
  const background = isMobilePlatform()
  const system = isAndroidPlatform() ? 'Android' : 'iOS'
  const storeEnabled = useNotificationStore((s) => s.notificationsEnabled)
  const setStoreEnabled = useNotificationStore((s) => s.setNotificationsEnabled)
  const settings = useLocalNotificationSettings()
  const setLocalEnabled = useSetLocalNotifications()
  const notificationsSupported = systemNotificationsSupported()
  const [blocked, setBlocked] = useState(() => 'Notification' in window && Notification.permission === 'denied')
  const [requestingPermission, setRequestingPermission] = useState(false)

  const enabled = notificationsSupported && (background ? settings.data?.enabled === true : storeEnabled)
  const saving = requestingPermission || setLocalEnabled.isPending
  const disabled = !notificationsSupported || saving || (background && settings.data === undefined)
  const description = background
    ? `${DESCRIPTION} Pipedash also checks in the background when ${system} allows it.`
    : DESCRIPTION
  let note: string | undefined
  if (!notificationsSupported) {
    note = notificationProblem('unsupported')
  } else if (background && settings.isError) {
    note = "Couldn't read the notification settings for this device."
  } else if (background && settings.data?.enabled && !settings.data.available) {
    note = settings.data.reason ?? undefined
  }

  const requestPermission = async (): Promise<boolean> => {
    setRequestingPermission(true)
    try {
      if (background && !(await confirmBackgroundAccess(system))) {
        return false
      }

      if (!(await requestSystemNotificationPermission())) {
        setBlocked(true)
        displayWarningNotification(notificationProblem('blocked'), 'Notifications blocked')

        return false
      }

      setBlocked(false)

      return true
    } finally {
      setRequestingPermission(false)
    }
  }

  const handleToggle = async (next: boolean): Promise<void> => {
    if (saving || (next && !(await requestPermission()))) {
      return
    }

    if (!background) {
      setStoreEnabled(next)

      return
    }

    const result = await setLocalEnabled.mutateAsync(next).catch(() => null)

    if (next && result && !result.enabled) {
      displayWarningNotification(result.reason ?? "Notifications couldn't be turned on.", 'Notifications are off')
    }
  }

  const pushCard = isMobilePlatform() && (
    <ErrorBoundary scope="feature" name="Push notifications">
      <PushNotificationsCard />
    </ErrorBoundary>
  )
  const devicesCard = !isTauri() && (
    <ErrorBoundary scope="feature" name="Mobile devices">
      <MobileDevicesCard />
    </ErrorBoundary>
  )

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

        {pushCard}
        {devicesCard}
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

      {pushCard}
      {devicesCard}
    </Stack>
  )
}
