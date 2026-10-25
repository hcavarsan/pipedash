import { Alert, Button, Group, Stack, Text } from '@mantine/core'

import { useBuildWatch } from '../../queries/useBuildWatchQueries'
import { mobileService } from '../../services/mobile'
import { STATUS_META } from '../../theme/status'
import type { PipelineStatus } from '../../types'
import { displayErrorNotification, errorMessage } from '../../utils/errorDisplay'

interface BuildWatchControlsProps {
  pipelineId: string
  runNumber: number
  status: PipelineStatus
}

export function BuildWatchControls({ pipelineId, runNumber, status }: BuildWatchControlsProps) {
  const watch = useBuildWatch()
  if (!watch.data?.supported || (status !== 'pending' && status !== 'running')) {
    return null
  }
  const current = watch.data.watch
  const watching = current?.monitoring === 'watching'
  const sameRun = watching && current.pipelineId === pipelineId && current.runNumber === runNumber

  return (
    <Stack gap="xs">
      <Button
        variant="default"
        size="sm"
        loading={watch.busy}
        disabled={!sameRun && (!watch.data.available || watching || watch.isError || watch.isFetching)}
        onClick={() => (sameRun ? watch.stop() : watch.start(pipelineId, runNumber))}
      >
        {sameRun ? 'Stop watching' : 'Watch run'}
      </Button>
      <Text size="xs" c="dimmed">
        {watching && !sameRun
          ? 'Stop the current watch before watching another run.'
          : !watch.data.available
            ? (watch.data.reason ?? 'Allow Live Activities and background execution in iOS Settings to watch runs.')
            : 'Watch on this device without a server. iOS may interrupt monitoring. Stopping a watch does not cancel the run.'}
      </Text>
      {!watch.data.available && (
        <Button
          variant="subtle"
          size="xs"
          onClick={() => {
            mobileService
              .openAppSettings()
              .catch((error: unknown) => displayErrorNotification(error, "Couldn't open Settings"))
          }}
        >
          Open Settings
        </Button>
      )}
    </Stack>
  )
}

interface BuildWatchPanelProps {
  onViewRun: (pipelineId: string, runNumber: number) => void
}

export function BuildWatchPanel({ onViewRun }: BuildWatchPanelProps) {
  const watch = useBuildWatch()
  if (!watch.enabled || (watch.data && !watch.data.supported)) {
    return null
  }
  if (watch.isError) {
    return (
      <Alert color="gray" title="Couldn't refresh Watch" m="xs">
        <Text size="sm">{errorMessage(watch.error, 'Try refreshing the current watch.')}</Text>
        <Button variant="subtle" size="xs" loading={watch.isFetching} onClick={() => void watch.refetch()}>
          Refresh Watch
        </Button>
        {watch.data?.watch?.monitoring === 'watching' && (
          <Button variant="subtle" size="xs" loading={watch.busy} onClick={watch.stop}>
            Stop watching
          </Button>
        )}
      </Alert>
    )
  }
  const current = watch.data?.watch
  if (!current) {
    return null
  }
  const monitoring = current.monitoring
  const title = {
    watching: 'Watching run',
    interrupted: 'Watch interrupted',
    completed: 'Watch completed',
    stopped: 'Watch stopped',
  }[current.monitoring]

  return (
    <Alert color="gray" title={title} m="xs">
      <Stack gap="xs">
        <Text size="sm" role="status">
          {current.pipelineName} #{current.runNumber} · {STATUS_META[current.status].label}
        </Text>
        <Text size="xs" c="dimmed">
          {monitoring === 'interrupted'
            ? 'Monitoring stopped before a final result. Open the run to refresh its status and start a new watch.'
            : monitoring === 'watching'
              ? 'iOS may interrupt monitoring. Stopping this watch does not cancel the provider run.'
              : monitoring === 'stopped'
                ? 'Monitoring stopped. The provider run was not cancelled.'
                : 'The provider reported a final result.'}
        </Text>
        {watch.data && !watch.data.available && watch.data.reason && (
          <Text size="xs" c="dimmed">
            {watch.data.reason}
          </Text>
        )}
        <Group gap="xs">
          <Button variant="subtle" size="xs" onClick={() => onViewRun(current.pipelineId, current.runNumber)}>
            View run #{current.runNumber}
          </Button>
          {monitoring === 'watching' && (
            <Button variant="subtle" size="xs" loading={watch.busy} onClick={watch.stop}>
              Stop watching
            </Button>
          )}
          {watch.data && !watch.data.available && (
            <Button
              variant="subtle"
              size="xs"
              onClick={() => {
                mobileService
                  .openAppSettings()
                  .catch((error: unknown) => displayErrorNotification(error, "Couldn't open Settings"))
              }}
            >
              Open Settings
            </Button>
          )}
        </Group>
      </Stack>
    </Alert>
  )
}
