import { Button, Card, Stack, Switch, Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import { Link } from 'react-router-dom'
import {
  useLocalNotificationSettings,
  useRemoveAlertHost,
  useSendTestAlert,
  useSetLocalNotifications,
} from '@/features/notifications/queries/useLocalNotificationQueries'
import { isMobileShell, useMobileShell } from '@/hooks/useIsMobile'
import { getConnection, hasLocalCore, isRemoteClient } from '@/services/connection'
import { isTauri } from '@/services/isTauri'
import { useNotificationStore } from '@/stores/notificationStore'
import type { AlertHostInfo } from '@/types/domain'
import { openConfirm } from '@/utils/confirm'
import { displayWarningNotification } from '@/utils/errorDisplay'
import { isAndroidPlatform, isMobilePlatform } from '@/utils/platform'
import {
  notificationProblem,
  requestSystemNotificationPermission,
  systemNotificationsSupported,
} from '@/utils/systemNotifications'

import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { SwitchRow } from '../../../components/ios/mobileParts'
import { MobileDevicesCard } from './MobileDevicesCard'
import { NotificationsBlockedRows } from './NotificationsBlockedRows'

const DESCRIPTION =
  'Get a system notification when a pipeline fails or runs longer than usual, and when a provider has an outage or recovers.'
const CONNECT_PATH = '/connect?purpose=alerts'
const CONNECT_SUBTITLE =
  "Get alerts from a Pipedash server or a computer that shares Pipedash, even when its pipelines aren't on this device."
const DUPLICATE_NOTE = 'If the same providers are also on this device, you can get the same alert twice.'

function confirmBackgroundAccess(system: string, host: string | null): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>()
  const access =
    host === null
      ? 'uses the provider credentials saved on it to check for failed pipelines, long-running runs, and provider outages'
      : `uses the access token it saved for ${host} to check for new alerts`

  openConfirm({
    title: 'Turn on notifications?',
    children: `While this device is locked, Pipedash ${access}. ${system} decides when these checks happen, so alerts can be delayed. ${system} asks to allow notifications next. Turning notifications off removes this access.`,
    labels: isMobileShell() ? { confirm: 'Turn On', cancel: 'Cancel' } : { confirm: 'Turn on', cancel: 'Cancel' },
    onConfirm: () => resolve(true),
    onCancel: () => resolve(false),
    onClose: () => resolve(false),
  })

  return promise
}

function confirmRemoveAlertHost(name: string, onConfirm: () => void): void {
  const device = isAndroidPlatform() ? 'This phone' : 'This iPhone'

  openConfirm({
    title: `Stop alerts from ${name}?`,
    children: `${device} stops getting alerts from ${name}, and ${name} forgets it.`,
    labels: { confirm: 'Remove', cancel: 'Cancel' },
    confirmProps: { color: 'red' },
    onConfirm,
  })
}

interface AlertHostGroupProps {
  mobileShell: boolean
  alertHost: AlertHostInfo | null
  removing: boolean
  onRemove: (name: string) => void
}

function AlertHostGroup({ mobileShell, alertHost, removing, onRemove }: AlertHostGroupProps): ReactNode {
  const status = alertHost?.push ? 'Push alerts on' : 'Checks in the background'

  if (mobileShell) {
    return (
      <IosGroup header="Alerts from a server" footer={alertHost ? DUPLICATE_NOTE : undefined}>
        {alertHost ? (
          <>
            <IosRow title={alertHost.name} subtitle={status} />
            <IosRow title="Remove" destructive disabled={removing} onClick={() => onRemove(alertHost.name)} />
          </>
        ) : (
          <IosRow title="Connect a server" subtitle={CONNECT_SUBTITLE} to={CONNECT_PATH} />
        )}
      </IosGroup>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="sm" align="flex-start">
        <Stack gap={0}>
          <Text size="sm" fw={500}>
            {alertHost ? alertHost.name : 'Alerts from a server'}
          </Text>
          <Text size="xs" c="dimmed">
            {alertHost ? status : CONNECT_SUBTITLE}
          </Text>
        </Stack>
        {alertHost ? (
          <>
            <Button variant="default" color="red" loading={removing} onClick={() => onRemove(alertHost.name)}>
              Remove
            </Button>
            <Text size="xs" c="dimmed">
              {DUPLICATE_NOTE}
            </Text>
          </>
        ) : (
          <Button component={Link} to={CONNECT_PATH} variant="default">
            Connect a server
          </Button>
        )}
      </Stack>
    </Card>
  )
}

export function NotificationsSection(): ReactNode {
  const mobileShell = useMobileShell()
  const client = isRemoteClient()
  const background = isMobilePlatform() && (hasLocalCore() || client)
  const host = getConnection().host ?? 'the server'
  const system = isAndroidPlatform() ? 'Android' : 'iOS'
  const storeEnabled = useNotificationStore((s) => s.notificationsEnabled)
  const setStoreEnabled = useNotificationStore((s) => s.setNotificationsEnabled)
  const settings = useLocalNotificationSettings()
  const setLocalEnabled = useSetLocalNotifications()
  const removeAlertHost = useRemoveAlertHost()
  const sendTestAlert = useSendTestAlert()
  const notificationsSupported = systemNotificationsSupported()
  const [blocked, setBlocked] = useState(() => 'Notification' in window && Notification.permission === 'denied')
  const [requestingPermission, setRequestingPermission] = useState(false)

  const alertHost = settings.data?.alertHost ?? null
  const enabled = notificationsSupported && (background ? settings.data?.enabled === true : storeEnabled)
  const saving = requestingPermission || setLocalEnabled.isPending
  const disabled = !notificationsSupported || saving || (background && settings.data === undefined)
  let description = DESCRIPTION
  if (background && client) {
    const delivery = alertHost?.push
      ? " Alerts arrive through Apple's push service. Pipedash's relay only sees an alert ID."
      : ` Pipedash checks ${host} in the background when ${system} allows it, so alerts can be delayed.`
    description = `Get a notification when a pipeline on ${host} fails or runs longer than usual, and when a provider has an outage or recovers.${delivery}`
  } else if (background) {
    description = `${DESCRIPTION} Pipedash also checks in the background when ${system} allows it.`
  }
  let note: string | undefined
  if (!notificationsSupported) {
    note = notificationProblem('unsupported')
  } else if (background && settings.isError) {
    note = "Couldn't read the notification settings for this device."
  } else if (background && settings.data?.enabled && !settings.data.available) {
    note = settings.data.reason ?? undefined
  } else if (background && settings.data?.pushProblem) {
    note = settings.data.pushProblem
  }

  const requestPermission = async (): Promise<boolean> => {
    setRequestingPermission(true)
    try {
      if (background && !(await confirmBackgroundAccess(system, client ? host : null))) {
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

  const alertHostGroup = background && !client && enabled && (
    <AlertHostGroup
      mobileShell={mobileShell}
      alertHost={alertHost}
      removing={removeAlertHost.isPending}
      onRemove={(name) => confirmRemoveAlertHost(name, () => removeAlertHost.mutate())}
    />
  )
  const canTest = background && alertHost?.push === true
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
          {canTest && (
            <IosRow title="Send test alert" disabled={sendTestAlert.isPending} onClick={() => sendTestAlert.mutate()} />
          )}
        </IosGroup>

        {blocked && notificationsSupported && (
          <IosGroup>
            <NotificationsBlockedRows />
          </IosGroup>
        )}

        {alertHostGroup}
        {devicesCard}
      </Stack>
    )
  }

  return (
    <Stack gap="md">
      <Card padding="md">
        <Stack gap="sm" align="flex-start">
          <Switch
            label="Notifications"
            description={note ?? description}
            checked={enabled}
            onChange={(e) => handleToggle(e.currentTarget.checked)}
            disabled={disabled}
          />
          {canTest && (
            <Button variant="default" loading={sendTestAlert.isPending} onClick={() => sendTestAlert.mutate()}>
              Send test alert
            </Button>
          )}
        </Stack>
      </Card>

      {alertHostGroup}
      {devicesCard}
    </Stack>
  )
}
