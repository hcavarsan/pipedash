import { Card, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { IosGroup } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import type { MobileDevice } from '../../../types/notifications'
import { openConfirm } from '../../../utils/confirm'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { useMobileDevices, useRemoveMobileDevice, useTestMobileDevice } from '../queries/usePushQueries'

import { PushDeviceRow } from './PushDeviceRow'

function pairingSubtitle(device: MobileDevice, now: number): string {
  const lastPush = device.last_success_at
    ? `last push ${formatRelativeTime(device.last_success_at, now)}`
    : 'no push delivered yet'
  return `Paired ${formatRelativeTime(device.created_at, now)} · ${lastPush}`
}

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

  if (data.subscriptions.length === 0) {
    return null
  }

  const confirmRemove = (device: MobileDevice): void => {
    openConfirm({
      title: `Remove ${device.device_name}?`,
      children: 'It stops receiving push alerts from this host.',
      labels: { confirm: 'Remove', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: () => remove.mutate(device),
    })
  }

  const rows = data.subscriptions.map((device) => (
    <PushDeviceRow
      key={device.id}
      title={device.device_name}
      subtitle={pairingSubtitle(device, now)}
      testLabel={`Send a test push to ${device.device_name}`}
      removeLabel={`Remove ${device.device_name}`}
      testing={sendTest.isPending && sendTest.variables === device.id}
      removing={remove.isPending && remove.variables?.id === device.id}
      onTest={() => sendTest.mutate(device.id)}
      onRemove={() => confirmRemove(device)}
    />
  ))

  if (mobileShell) {
    return (
      <IosGroup
        header="Older push pairings"
        footer="These iPhones and iPads use push pairing from an older Pipedash version. They get a push when a pipeline fails. Newer devices pair in Settings → Devices."
      >
        {rows}
      </IosGroup>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            Older push pairings
          </Text>
          <Text size="xs" c="dimmed">
            These iPhones and iPads use push pairing from an older Pipedash version. They get a push when a pipeline
            fails. Newer devices pair in Settings → Devices.
          </Text>
        </Stack>

        <Stack gap="sm">{rows}</Stack>
      </Stack>
    </Card>
  )
}
