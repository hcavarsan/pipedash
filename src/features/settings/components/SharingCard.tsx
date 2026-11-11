import { Alert, Button, Card, Code, Group, Stack, Switch, Text } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { CopyButton } from '@/components/atoms/CopyButton'
import { QueryBoundary } from '@/components/common/QueryBoundary'
import {
  useResetSharingPort,
  useSetSharing,
  useSharingStatus,
} from '@/features/connection/queries/useConnectionQueries'
import type { SharingStatus } from '@/types/native'

interface SharingRecoveryProps {
  pending: boolean
  onReset: () => void
}

function SharingRecovery({ pending, onReset }: SharingRecoveryProps): ReactNode {
  return (
    <>
      <Button size="xs" variant="default" loading={pending} onClick={onReset}>
        Use a new port
      </Button>
      <Text size="xs" c="dimmed">
        Paired devices need to pair again.
      </Text>
    </>
  )
}

function SharingControls({ status }: { status: SharingStatus }): ReactNode {
  const setSharing = useSetSharing()
  const resetPort = useResetSharingPort()
  const recovery =
    status.errorCode === 'PORT_IN_USE' ? (
      <SharingRecovery pending={resetPort.isPending} onReset={() => resetPort.mutate()} />
    ) : null
  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            Share with other devices
          </Text>
          <Text size="xs" c="dimmed">
            Your phone or another computer can show the pipelines from this computer. Keep this computer on and Pipedash
            open.
          </Text>
        </Stack>

        <Switch
          label="Share on this network"
          checked={status.enabled}
          disabled={setSharing.isPending}
          onChange={(event) => setSharing.mutate(event.currentTarget.checked)}
        />

        <Text size="xs" c="dimmed">
          Devices on your network connect without encryption. Each one needs a pairing code first, and you can remove it
          at any time.
        </Text>

        <Text size="xs" c="dimmed">
          To send push alerts to paired iPhones and iPads, turn on Background notifications in Notifications.
        </Text>

        {status.running && status.url && (
          <Group gap="xs" wrap="nowrap">
            <Text size="sm">Sharing at</Text>
            <Code>{status.url}</Code>
            <CopyButton value={status.url} label="Copy address" />
          </Group>
        )}

        {status.error && (
          <Alert color="orange" title="Couldn't share Pipedash" icon={<IconAlertTriangle size={16} aria-hidden />}>
            <Stack gap="xs" align="flex-start">
              <Text size="sm">{status.error}</Text>
              {recovery}
            </Stack>
          </Alert>
        )}
      </Stack>
    </Card>
  )
}

export function SharingCard(): ReactNode {
  return (
    <QueryBoundary
      query={useSharingStatus({ enabled: true })}
      loading="Loading sharing status…"
      failure="Couldn't load sharing status"
    >
      {(status) => <SharingControls status={status} />}
    </QueryBoundary>
  )
}
