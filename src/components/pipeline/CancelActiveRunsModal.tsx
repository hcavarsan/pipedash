import { Button, Group, Stack, Text } from '@mantine/core'
import { useEffect, useMemo, useState } from 'react'

import { PAGE_SIZES } from '../../constants/pagination'
import { usePlugins } from '../../contexts/PluginContext'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { service } from '../../services'
import type { Pipeline } from '../../types'
import {
  buildCancelGroups,
  type CancelGroup,
  cancellableGroups,
  countRuns,
  findUnfinishedRuns,
  type UnfinishedRuns,
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

const IDLE = { for: [] as Pipeline[], done: 0, result: null as UnfinishedRuns | null }

export const CancelActiveRunsModal = ({ opened, scope, onClose, onConfirm }: CancelActiveRunsModalProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const { getPluginByType, getPluginDisplayName } = usePlugins()
  const [pending, setPending] = useState(false)

  const { candidates, skippedNote } = useMemo(() => {
    const cancelCapable = (pipeline: Pipeline) => Boolean(getPluginByType(pipeline.provider_type)?.capabilities.cancel)
    const pipelines = scope?.pipelines ?? []
    const skipped = new Map<string, number>()

    for (const pipeline of unfinishedPipelines(pipelines)) {
      if (!cancelCapable(pipeline)) {
        const name = getPluginDisplayName(pipeline.provider_type)

        skipped.set(name, (skipped.get(name) ?? 0) + 1)
      }
    }

    return {
      candidates: pipelines.filter(cancelCapable),
      skippedNote: [...skipped]
        .map(
          ([name, total]) =>
            `Skipped ${formatCount(total, 'running pipeline')} on ${name}: it can't cancel runs from Pipedash.`
        )
        .join(' '),
    }
  }, [scope, getPluginByType, getPluginDisplayName])
  const [found, setFound] = useState<{ for: Pipeline[]; done: number; result: UnfinishedRuns | null }>(IDLE)

  useEffect(() => {
    if (!opened || candidates.length === 0) {
      return
    }

    let stale = false

    setFound({ for: candidates, done: 0, result: null })
    findUnfinishedRuns(
      candidates,
      (pipelineId, page) =>
        stale
          ? Promise.reject(new Error('Stale search'))
          : service.fetchRunHistory(pipelineId, page, PAGE_SIZES.RUN_HISTORY),
      undefined,
      (done) => !stale && setFound((current) => ({ ...current, done }))
    ).then((result) => !stale && setFound({ for: candidates, done: candidates.length, result }))

    return () => {
      stale = true
    }
  }, [opened, candidates])

  const search = found.for === candidates ? found : IDLE
  const loading = candidates.length > 0 && search.result === null
  const unreadable = search.result?.failed.map((pipeline) => pipeline.name) ?? []
  const groups = buildCancelGroups(candidates, search.result?.runs ?? {}, (pipeline) => ({
    name: getPluginDisplayName(pipeline.provider_type),
    canCancel: Boolean(getPluginByType(pipeline.provider_type)?.capabilities.cancel),
  }))
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
    <IosGroup key={group.pipeline.id} header={`${group.pipeline.name} · ${group.name}`}>
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
        <LoadingState
          variant="compact"
          message={`Looking for running runs… (${search.done} of ${formatCount(candidates.length, 'pipeline')})`}
        />
      ) : (
        <Stack gap={mobileShell ? 'lg' : 'md'}>
          {groups.length === 0 && unreadable.length === 0 && (
            <Text size="sm">{skippedNote ? `No runs to cancel in ${label}.` : `Nothing is running in ${label}.`}</Text>
          )}
          {mobileShell ? shellBody : desktopBody}
          {unreadable.length > 0 && (
            <Text size="xs" c="red">
              Couldn't load runs for {unreadable.join(', ')}.
            </Text>
          )}
          {skippedNote && (
            <Text size="xs" c="orange">
              {skippedNote}
            </Text>
          )}
        </Stack>
      )}
    </StandardModal>
  )
}
