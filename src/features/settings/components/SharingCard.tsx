import { Alert, Button, Card, Code, Group, Stack, Switch, Text } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { CopyButton } from '@/components/atoms/CopyButton'
import { ErrorState } from '@/components/common/ErrorState'
import { LoadingState } from '@/components/common/LoadingState'
import {
  useResetSharingPort,
  useSetSharing,
  useSharingStatus,
} from '@/features/connection/queries/useConnectionQueries'

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

export function SharingCard(): ReactNode {
  const { data: status, error, isLoading, isRefetching, refetch } = useSharingStatus({ enabled: true })
  const setSharing = useSetSharing()
  const resetPort = useResetSharingPort()

  if (error) {
    return (
      <ErrorState
        title="Couldn't load sharing status"
        error={error}
        onRetry={() => refetch()}
        retrying={isRefetching}
      />
    )
  }

  const recovery =
    status?.errorCode === 'PORT_IN_USE' ? (
      <SharingRecovery pending={resetPort.isPending} onReset={() => resetPort.mutate()} />
    ) : null

  if (isLoading || !status) {
    return <LoadingState message="Loading sharing status" />
  }

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
          Phones that turn on notifications also get push alerts through relay.pipedash.app. The relay sees only an
          alert ID.
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
