import { Alert, Card, Stack, Text } from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { useMobileDevices, useRemoveMobileDevice, useTestMobileDevice } from '../queries/usePushQueries'

import { PushDeviceRow } from './PushDeviceRow'

export function MobileDevicesCard(): ReactNode {
  const { data, isLoading, error, refetch, isRefetching } = useMobileDevices()
  const mobileShell = useMobileShell()
  const remove = useRemoveMobileDevice()
  const sendTest = useTestMobileDevice()
  const now = useNow(60_000)

  if (error) {
    return (
      <ErrorState
        title="Couldn't load mobile devices"
        error={error}
        onRetry={() => refetch()}
        retrying={isRefetching}
      />
    )
  }

  if (isLoading || !data) {
    return <LoadingState message="Loading mobile devices..." />
  }

  if (mobileShell) {
    return (
      <IosGroup
        header="Mobile devices"
        footer={
          data.relay_configured
            ? 'iPhones and iPads paired with this server get a push when a pipeline fails.'
            : 'iPhones and iPads paired with this server get a push when a pipeline fails. This server has no push relay. Set push.relay_url in the config file or PIPEDASH_PUSH_RELAY_URL, then restart it.'
        }
      >
        {data.subscriptions.length === 0 ? (
          <IosRow
            title="No devices paired yet"
            subtitle="Open Settings > Notifications in the Pipedash iOS app to pair this server."
          />
        ) : (
          data.subscriptions.map((device) => (
            <PushDeviceRow
              key={device.id}
              title={device.device_name}
              subtitle={`Paired ${formatRelativeTime(device.created_at, now)} · ${
                device.last_success_at
                  ? `last push ${formatRelativeTime(device.last_success_at, now)}`
                  : 'no push delivered yet'
              }`}
              testLabel={`Send a test push to ${device.device_name}`}
              removeLabel={`Remove ${device.device_name}`}
              testing={sendTest.isPending && sendTest.variables === device.id}
              removing={remove.isPending && remove.variables === device.id}
              onTest={() => sendTest.mutate(device.id)}
              onRemove={() => remove.mutate(device.id)}
            />
          ))
        )}
      </IosGroup>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            Mobile devices
          </Text>
          <Text size="xs" c="dimmed">
            iPhones and iPads paired with this server get a push when a pipeline fails.
          </Text>
        </Stack>

        {!data.relay_configured && (
          <Alert icon={<IconInfoCircle size={16} aria-hidden />} color="gray" variant="light">
            This server has no push relay. Set push.relay_url in the config file or PIPEDASH_PUSH_RELAY_URL, then
            restart it.
          </Alert>
        )}

        {data.subscriptions.length === 0 ? (
          <Text size="sm" c="dimmed">
            No devices paired yet. Open Settings &gt; Notifications in the Pipedash iOS app to pair this server.
          </Text>
        ) : (
          <Stack gap="sm">
            {data.subscriptions.map((device) => (
              <PushDeviceRow
                key={device.id}
                title={device.device_name}
                subtitle={`Paired ${formatRelativeTime(device.created_at, now)} · ${
                  device.last_success_at
                    ? `last push ${formatRelativeTime(device.last_success_at, now)}`
                    : 'no push delivered yet'
                }`}
                testLabel={`Send a test push to ${device.device_name}`}
                removeLabel={`Remove ${device.device_name}`}
                testing={sendTest.isPending && sendTest.variables === device.id}
                removing={remove.isPending && remove.variables === device.id}
                onTest={() => sendTest.mutate(device.id)}
                onRemove={() => remove.mutate(device.id)}
              />
            ))}
          </Stack>
        )}
      </Stack>
    </Card>
  )
}
