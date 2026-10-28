import { ActionIcon, Alert, Button, Card, Group, Menu, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconDots, IconEye } from '@tabler/icons-react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useBuildWatch } from '../../queries/useBuildWatchQueries'
import { mobileService } from '../../services/mobile'
import { STATUS_META } from '../../theme/status'
import type { PipelineStatus } from '../../types'
import { displayErrorNotification, errorMessage } from '../../utils/errorDisplay'
import { StatusIcon } from '../atoms/StatusBadge'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup } from '../ios/IosList'
import { IosPipelineRow } from '../table/IosPipelineList'
import { SectionHeading } from '../table/PipelineRowParts'

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
        mih={44}
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
            ? (watch.data.reason ?? 'Background monitoring is unavailable on this device.')
            : 'Watch on this device without a server. iOS may interrupt monitoring. Stopping a watch does not cancel the run.'}
      </Text>
      {!watch.data.available && (
        <Button
          variant="subtle"
          size="xs"
          mih={44}
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

interface BuildWatchSectionProps {
  onViewRun: (pipelineId: string, runNumber: number) => void
}

export function BuildWatchSection({ onViewRun }: BuildWatchSectionProps) {
  const watch = useBuildWatch()
  const mobileShell = useMobileShell()
  if (!watch.enabled || (watch.data && !watch.data.supported)) {
    return null
  }
  const current = watch.data?.watch
  if (!current && !watch.isError) {
    return null
  }
  const state = current
    ? {
        watching: null,
        interrupted: 'Watch interrupted',
        completed: 'Watch completed',
        stopped: 'Watch stopped',
      }[current.monitoring]
    : null
  const actions = current && (
    <Menu position="bottom">
      <Menu.Target>
        <ActionIcon size={44} data-row-menu aria-label="Watched run actions">
          <IconDots size={22} />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Item onClick={() => onViewRun(current.pipelineId, current.runNumber)}>
          View run #{current.runNumber}
        </Menu.Item>
        {current.monitoring === 'watching' && (
          <Menu.Item disabled={watch.busy} onClick={watch.stop}>
            Stop watching
          </Menu.Item>
        )}
        {watch.data && !watch.data.available && (
          <Menu.Item
            onClick={() => {
              mobileService
                .openAppSettings()
                .catch((error: unknown) => displayErrorNotification(error, "Couldn't open Settings"))
            }}
          >
            Open Settings
          </Menu.Item>
        )}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
  const row =
    current &&
    (mobileShell ? (
      <IosPipelineRow
        status={current.status}
        title={`${current.pipelineName} #${current.runNumber}`}
        subtitle={[current.providerName, current.branch].filter(Boolean).join(' · ')}
        detail={state ?? undefined}
        hint={`View run #${current.runNumber}`}
        onClick={() => onViewRun(current.pipelineId, current.runNumber)}
        trailing={actions}
      />
    ) : (
      <Card padding="sm" className="interactive-surface stretched">
        <Stack gap="xs" h="100%" justify="space-between">
          <Group justify="space-between" align="flex-start" wrap="nowrap" gap="sm">
            <UnstyledButton
              className="item-primary"
              miw={0}
              mih={44}
              flex={1}
              aria-label={`View run #${current.runNumber}`}
              onClick={() => onViewRun(current.pipelineId, current.runNumber)}
            >
              <Text size="sm" fw={600} truncate>
                {current.pipelineName} #{current.runNumber}
              </Text>
              <Text size="xs" c="dimmed" truncate>
                {STATUS_META[current.status].label}
                {state && ` · ${state}`}
              </Text>
            </UnstyledButton>
            <StatusIcon status={current.status} />
          </Group>
          <Group justify="space-between" wrap="nowrap" gap="sm">
            <Text size="xs" c="dimmed" truncate>
              {[current.providerName, current.branch].filter(Boolean).join(' · ')}
            </Text>
            {actions}
          </Group>
        </Stack>
      </Card>
    ))
  const recovery = watch.isError && (
    <Alert color="gray" title="Couldn't refresh Watch">
      <Text size="sm">{errorMessage(watch.error, 'Try refreshing the current watch.')}</Text>
      <Button variant="subtle" size="xs" mih={44} loading={watch.isFetching} onClick={() => void watch.refetch()}>
        Refresh Watch
      </Button>
    </Alert>
  )

  return mobileShell ? (
    <IosGroup header="Watching">
      {row}
      {recovery}
    </IosGroup>
  ) : (
    <Stack gap="sm" component="section" aria-labelledby="watching-run-title">
      <SectionHeading
        id="watching-run-title"
        icon={<IconEye size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
        label="Watching"
        count={current ? 1 : 0}
      />
      {row && (
        <SimpleGrid cols={{ base: 1, sm: 2, lg: 3, xl: 4 }} spacing="sm">
          {row}
        </SimpleGrid>
      )}
      {recovery}
    </Stack>
  )
}
