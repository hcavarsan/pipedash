import {
  ActionIcon,
  Alert,
  Button,
  Card,
  Group,
  Menu,
  SimpleGrid,
  Stack,
  Text,
  UnstyledButton,
  VisuallyHidden,
} from '@mantine/core'
import { IconDots, IconEye } from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'
import { Link, type NavigateFunction, useNavigate } from 'react-router-dom'

import { useDesktopStatus } from '../../features/settings/queries/useDesktopQueries'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useBuildWatch } from '../../queries/useBuildWatchQueries'
import { mobileService } from '../../services/mobile'
import { STATUS_META } from '../../theme/status'
import type { PipelineStatus } from '../../types/domain'
import { displayErrorNotification, errorMessage } from '../../utils/errorDisplay'
import { isDesktopTauri } from '../../utils/platform'
import { StatusIcon } from '../atoms/StatusBadge'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup } from '../ios/IosList'
import { IosPipelineRow } from './IosPipelineList'
import { SectionHeading } from './PipelineRowParts'

function openWatchSettings(navigate: NavigateFunction): void {
  if (isDesktopTauri()) {
    void navigate('/settings/general')
    return
  }
  mobileService.openAppSettings().catch((error: unknown) => displayErrorNotification(error, "Couldn't open Settings"))
}

function BackgroundWatchGuidance({ watching }: { watching: boolean }): ReactNode {
  const desktop = useDesktopStatus()
  if (!isDesktopTauri() || !watching || desktop.data?.settings.keep_running_on_close !== false) {
    return null
  }

  return (
    <Text size="xs" c="dimmed">
      Background mode is off. Keep Pipedash open to watch this run, or turn on background mode in{' '}
      <Link to="/settings/general">General settings</Link>.
    </Text>
  )
}

interface BuildWatchControlsProps {
  pipelineId: string
  runNumber: number
  status: PipelineStatus
}

export function BuildWatchControls({ pipelineId, runNumber, status }: BuildWatchControlsProps): ReactNode {
  const watch = useBuildWatch()
  const navigate = useNavigate()
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
            : isDesktopTauri()
              ? 'Watch this run while Pipedash is running. Closing the window keeps watching only when background mode is on. Stopping a watch does not cancel the run.'
              : 'Watch on this device without a server. iOS may interrupt monitoring. Stopping a watch does not cancel the run.'}
      </Text>
      {!watch.data.available && (
        <Button variant="subtle" size="xs" mih={44} onClick={() => openWatchSettings(navigate)}>
          Open Settings
        </Button>
      )}
      <BackgroundWatchGuidance watching={watching} />
    </Stack>
  )
}

interface BuildWatchSectionProps {
  onViewRun: (pipelineId: string, runNumber: number) => void
}

export function BuildWatchSection({ onViewRun }: BuildWatchSectionProps): ReactNode {
  const watch = useBuildWatch()
  const mobileShell = useMobileShell()
  const navigate = useNavigate()
  const titleId = useId()
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
          <IconDots size={22} aria-hidden />
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
          <Menu.Item onClick={() => openWatchSettings(navigate)}>Open Settings</Menu.Item>
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
        subtitle={[state, current.providerName, current.branch].filter(Boolean).join(' · ')}
        hint={`View run #${current.runNumber}`}
        longPressMenu
        onClick={() => onViewRun(current.pipelineId, current.runNumber)}
        menu={actions}
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

  const live = (
    <VisuallyHidden role="status">
      {current
        ? `${current.pipelineName} #${current.runNumber}: ${STATUS_META[current.status].label}${state ? `, ${state}` : ''}`
        : ''}
    </VisuallyHidden>
  )

  return mobileShell ? (
    <IosGroup header="Watching">
      {row}
      {recovery}
      {live}
    </IosGroup>
  ) : (
    <Stack gap="sm" component="section" aria-labelledby={titleId}>
      <SectionHeading
        id={titleId}
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
      <BackgroundWatchGuidance watching={current?.monitoring === 'watching'} />
      {live}
    </Stack>
  )
}
