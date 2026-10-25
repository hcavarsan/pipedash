import { Card, Divider, Stack, Switch, Title } from '@mantine/core'
import { useState } from 'react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { isTauri } from '@/services'
import { useNotificationStore } from '@/stores/notificationStore'
import { displayWarningNotification } from '@/utils/errorDisplay'
import { isMobilePlatform } from '@/utils/platform'
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

export function NotificationsSection() {
  const mobileShell = useMobileShell()
  const failureNotificationsEnabled = useNotificationStore((s) => s.failureNotificationsEnabled)
  const setFailureNotificationsEnabled = useNotificationStore((s) => s.setFailureNotificationsEnabled)
  const providerAlertsEnabled = useNotificationStore((s) => s.providerAlertsEnabled)
  const setProviderAlertsEnabled = useNotificationStore((s) => s.setProviderAlertsEnabled)
  const notificationsSupported = systemNotificationsSupported()
  const [blocked, setBlocked] = useState(() => 'Notification' in window && Notification.permission === 'denied')

  const handleNotificationToggle = async (enabled: boolean, setEnabled: (enabled: boolean) => void) => {
    if (!enabled) {
      setEnabled(false)

      return
    }

    if (!(await requestSystemNotificationPermission())) {
      setBlocked(true)
      displayWarningNotification(notificationProblem('blocked'), 'Notifications blocked')

      return
    }

    setBlocked(false)
    setEnabled(true)
  }

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <IosGroup footer={notificationsSupported ? undefined : notificationProblem('unsupported')}>
          <SwitchRow
            title="Notify when a pipeline fails"
            subtitle="Show a system notification when a refresh finds a pipeline that just failed."
            checked={notificationsSupported && failureNotificationsEnabled}
            onChange={(e) => handleNotificationToggle(e.currentTarget.checked, setFailureNotificationsEnabled)}
            disabled={!notificationsSupported}
          />
          <SwitchRow
            title="Notify me about provider outages"
            subtitle="Show a system notification when a provider reports an outage or recovers."
            checked={notificationsSupported && providerAlertsEnabled}
            onChange={(e) => handleNotificationToggle(e.currentTarget.checked, setProviderAlertsEnabled)}
            disabled={!notificationsSupported}
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
    <>
      <Title order={2} size="lg" fw={600} mb="lg">
        Notifications
      </Title>

      <Stack gap="md">
        <Card padding="md">
          <Stack gap="md">
            <Switch
              label="Notify when a pipeline fails"
              description={
                notificationsSupported
                  ? 'Show a system notification when a refresh finds a pipeline that just failed.'
                  : notificationProblem('unsupported')
              }
              checked={notificationsSupported && failureNotificationsEnabled}
              onChange={(e) => handleNotificationToggle(e.currentTarget.checked, setFailureNotificationsEnabled)}
              disabled={!notificationsSupported}
            />

            <Divider />

            <Switch
              label="Notify me about provider outages"
              description={
                notificationsSupported
                  ? 'Show a system notification when a provider reports an outage or recovers.'
                  : notificationProblem('unsupported')
              }
              checked={notificationsSupported && providerAlertsEnabled}
              onChange={(e) => handleNotificationToggle(e.currentTarget.checked, setProviderAlertsEnabled)}
              disabled={!notificationsSupported}
            />
          </Stack>
        </Card>

        {isMobilePlatform() && <PushNotificationsCard />}
        {!isTauri() && <MobileDevicesCard />}
      </Stack>
    </>
  )
}
