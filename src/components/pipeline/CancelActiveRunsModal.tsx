import { Button, Group, Stack, Text } from '@mantine/core'
import { useQueries } from '@tanstack/react-query'
import { useState } from 'react'

import { PAGE_SIZES } from '../../constants/pagination'
import { usePlugins } from '../../contexts/PluginContext'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { GC_TIMES } from '../../lib/cacheConfig'
import { queryKeys } from '../../lib/queryKeys'
import { service } from '../../services'
import type { Pipeline } from '../../types'
import {
  buildCancelGroups,
  type CancelGroup,
  cancellableGroups,
  countRuns,
  unfinishedPipelines,
} from '../../utils/cancelActiveRuns'
import { formatCount } from '../../utils/formatCount'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { StatusBadge } from '../atoms/StatusBadge'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'

export interface CancelScope {
  label: string
  pipelines: Pipeline[]
}

interface CancelActiveRunsModalProps {
  opened: boolean
  scope: CancelScope | null
  onClose: () => void
  onConfirm: (groups: CancelGroup[]) => Promise<unknown>
}

const skippedNote = (group: CancelGroup) =>
  `${group.name} can't cancel runs from Pipedash. ${formatCount(group.runs.length, 'run')} skipped.`

export const CancelActiveRunsModal = ({ opened, scope, onClose, onConfirm }: CancelActiveRunsModalProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const { getPluginByType, getPluginDisplayName } = usePlugins()
  const [pending, setPending] = useState(false)

  const candidates = scope ? unfinishedPipelines(scope.pipelines) : []
  const results = useQueries({
    queries: candidates.map((pipeline) => ({
      queryKey: queryKeys.runs.list(pipeline.id, 1),
      queryFn: () => service.fetchRunHistory(pipeline.id, 1, PAGE_SIZES.RUN_HISTORY),
      staleTime: 0,
      gcTime: GC_TIMES.MEDIUM,
      enabled: opened,
    })),
  })

  const loading = results.some((result) => result.isFetching)
  const unreadable = candidates.filter((_, index) => results[index].isError).map((pipeline) => pipeline.name)
  const groups = buildCancelGroups(
    candidates,
    Object.fromEntries(candidates.map((pipeline, index) => [pipeline.id, results[index].data?.runs ?? []])),
    (pipeline) => ({
      name: getPluginDisplayName(pipeline.provider_type),
      canCancel: Boolean(getPluginByType(pipeline.provider_type)?.capabilities.cancel),
    })
  )
  const targets = cancellableGroups(groups)
  const count = countRuns(targets)
  const label = scope?.label ?? ''
  const ready = !loading && count > 0

  const confirm = async () => {
    setPending(true)

    try {
      await onConfirm(targets)
    } finally {
      setPending(false)
      onClose()
    }
  }

  const footer = ready
    ? (requestClose: () => void) => (
        <Group justify="flex-end" gap="sm" grow={isMobile || mobileShell}>
          <Button variant="default" size="sm" onClick={requestClose} disabled={pending}>
            Keep running
          </Button>
          <Button color="red" size="sm" onClick={confirm} loading={pending}>
            Cancel {formatCount(count, 'run')}
          </Button>
        </Group>
      )
    : undefined

  const shellBody = groups.map((group) => (
    <IosGroup
      key={group.pipeline.id}
      header={`${group.pipeline.name} · ${group.name}`}
      footer={group.canCancel ? undefined : skippedNote(group)}
    >
      {group.runs.map((run) => (
        <IosRow
          key={run.run_number}
          title={`#${run.run_number}${run.branch ? ` · ${run.branch}` : ''}`}
          subtitle={`Started ${formatRelativeTime(run.started_at, now)}`}
          value={<StatusBadge status={run.status} />}
        />
      ))}
    </IosGroup>
  ))

  const desktopBody = groups.map((group) => (
    <Stack key={group.pipeline.id} gap="xs">
      <Group justify="space-between" wrap="nowrap" gap="md">
        <Text size="sm" fw={600} truncate>
          {group.pipeline.name}
        </Text>
        <Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
          {group.name}
        </Text>
      </Group>
      {!group.canCancel && (
        <Text size="xs" c="orange">
          {skippedNote(group)}
        </Text>
      )}
      {group.runs.map((run) => (
        <Group key={run.run_number} justify="space-between" wrap="nowrap" gap="md">
          <Group gap="sm" wrap="nowrap" miw={0}>
            <Text size="sm" fw={500}>
              #{run.run_number}
            </Text>
            {run.branch && (
              <Text size="sm" c="dimmed" truncate>
                {run.branch}
              </Text>
            )}
          </Group>
          <Group gap="sm" wrap="nowrap" style={{ flexShrink: 0 }}>
            <StatusBadge status={run.status} />
            <Text size="xs" c="dimmed">
              {formatRelativeTime(run.started_at, now)}
            </Text>
          </Group>
        </Group>
      ))}
    </Stack>
  ))

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      size="md"
      title={ready ? `Cancel ${formatCount(count, 'run')} in ${label}` : `Cancel running in ${label}`}
      loading={pending}
      footer={footer}
    >
      {loading ? (
        <LoadingState variant="compact" message="Looking for running runs…" />
      ) : groups.length === 0 ? (
        <Text size="sm">
          {unreadable.length > 0
            ? `Couldn't load runs for ${unreadable.join(', ')}.`
            : `Nothing is running in ${label}.`}
        </Text>
      ) : (
        <Stack gap={mobileShell ? 'lg' : 'md'}>
          {mobileShell ? shellBody : desktopBody}
          {unreadable.length > 0 && (
            <Text size="xs" c="red">
              Couldn't load runs for {unreadable.join(', ')}.
            </Text>
          )}
        </Stack>
      )}
    </StandardModal>
  )
}
