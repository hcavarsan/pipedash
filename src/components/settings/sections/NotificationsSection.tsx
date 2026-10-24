import { Card, Divider, Stack, Switch, Title } from '@mantine/core'
import { notifications } from '@mantine/notifications'

import { isTauri } from '@/services'
import { useNotificationStore } from '@/stores/notificationStore'
import { isMobilePlatform } from '@/utils/platform'
import {
  notificationProblem,
  requestSystemNotificationPermission,
  systemNotificationsSupported,
} from '@/utils/systemNotifications'

import { MobileDevicesCard } from '../MobileDevicesCard'
import { PushNotificationsCard } from '../PushNotificationsCard'

export function NotificationsSection() {
  const failureNotificationsEnabled = useNotificationStore((s) => s.failureNotificationsEnabled)
  const setFailureNotificationsEnabled = useNotificationStore((s) => s.setFailureNotificationsEnabled)
  const providerAlertsEnabled = useNotificationStore((s) => s.providerAlertsEnabled)
  const setProviderAlertsEnabled = useNotificationStore((s) => s.setProviderAlertsEnabled)
  const notificationsSupported = systemNotificationsSupported()

  const handleNotificationToggle = async (enabled: boolean, setEnabled: (enabled: boolean) => void) => {
    if (!enabled) {
      setEnabled(false)

      return
    }

    if (!(await requestSystemNotificationPermission())) {
      notifications.show({
        title: 'Notifications blocked',
        message: notificationProblem('blocked'),
        color: 'orange',
      })

      return
    }

    setEnabled(true)
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
