import { Button, Card, Stack, Switch, Text } from '@mantine/core'
import { useMutation } from '@tanstack/react-query'
import { platform } from '@tauri-apps/plugin-os'
import type { ReactNode } from 'react'

import { ErrorState } from '../../../components/common/ErrorState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { notifications } from '../../../lib/notifications'
import { desktopService } from '../../../services/desktop'
import { useDesktopStatus, useUpdateDesktopSettings } from '../../settings/queries/useDesktopQueries'

export function DesktopNotificationsCard(): ReactNode {
  const status = useDesktopStatus()
  const settings = useUpdateDesktopSettings()
  const test = useMutation({
    mutationFn: () => desktopService.testNotification(),
    onSuccess: () =>
      notifications.show({
        title: 'Notification submitted',
        message: 'Your operating system decides whether to display it.',
        color: 'green',
      }),
  })
  const enabled = status.data?.settings.notifications_enabled ?? false
  const error = settings.error ?? status.error ?? status.data?.error
  const os = platform()
  const guidance =
    os === 'macos'
      ? 'Allow Pipedash in System Settings > Notifications'
      : os === 'windows'
        ? 'Allow Pipedash in Settings > System > Notifications'
        : "Allow Pipedash in your desktop's notification settings"

  return (
    <Card padding="md">
      <Stack gap="md">
        <LiveStatus message={settings.busy ? 'Saving notification settings' : ''} />
        <Switch
          label="System notifications"
          aria-label="System notifications"
          description="Get alerts for pipelines, watched runs and provider outages while Pipedash is running."
          checked={enabled}
          disabled={!status.data || settings.busy}
          onChange={(event) => settings.mutate({ notifications_enabled: event.currentTarget.checked })}
        />
        <Text size="xs" c="dimmed">
          {guidance}. Do Not Disturb can suppress display.
        </Text>
        <Text size="xs" c="dimmed">
          Notifications stay in the app for this session even when system notifications are off.
        </Text>
        {os === 'windows' && (
          <Text size="xs" c="dimmed">
            Install Pipedash for its notification identity. Open a notification while its toast is visible. Windows
            Action Center cannot reopen Pipedash after the toast disappears. Use the app or native menu instead.
          </Text>
        )}
        {error && (
          <ErrorState
            title="Couldn't load or save notification settings"
            error={error}
            onRetry={() => settings.mutate({})}
            retrying={settings.busy}
          />
        )}
        <Button
          variant="default"
          size="sm"
          disabled={!enabled || !status.data?.ready || settings.busy}
          loading={test.isPending}
          onClick={() => test.mutate()}
        >
          Send test notification
        </Button>
        {test.error && <ErrorState title="Couldn't submit test notification" error={test.error} />}
      </Stack>
    </Card>
  )
}
