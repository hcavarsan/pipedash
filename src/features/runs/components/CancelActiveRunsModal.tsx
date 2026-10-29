import { Button, Group, Stack, Text } from '@mantine/core'
import { type ReactNode, useActionState, useId, useMemo } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import { usePlugins } from '../../../queries/usePluginsQueries'
import { useUnfinishedRuns } from '../../../queries/useRunHistoryQueries'
import type { Pipeline } from '../../../types/domain'
import { actionTarget } from '../../../utils/actionTarget'
import {
  buildCancelGroups,
  type CancelGroup,
  cancellableGroups,
  countRuns,
  unfinishedPipelines,
} from '../../../utils/cancelActiveRuns'
import { errorMessage } from '../../../utils/errorDisplay'
import { formatCount } from '../../../utils/formatCount'
import { getPluginByType, getPluginDisplayName } from '../../../utils/pluginHelpers'

import { CancelRunTargets } from './CancelRunTargets'

interface CancelActionState {
  error: string | null
}

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

export const CancelActiveRunsModal = ({ opened, scope, onClose, onConfirm }: CancelActiveRunsModalProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const { data: plugins } = usePlugins()
  const formId = useId()

  const { candidates, skippedNote } = useMemo(() => {
    const cancelCapable = (pipeline: Pipeline) =>
      Boolean(getPluginByType(plugins, pipeline.provider_type)?.capabilities.cancel)
    const pipelines = scope?.pipelines ?? []
    const skipped = new Map<string, number>()

    for (const pipeline of unfinishedPipelines(pipelines)) {
      if (!cancelCapable(pipeline)) {
        const name = getPluginDisplayName(plugins, pipeline.provider_type)

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
  }, [scope, plugins])
  const search = useUnfinishedRuns(candidates, opened)
  const loading = search.isFetching
  const unreadable = search.data?.failed.map((entry) => entry.name) ?? []
  const groups = useMemo(
    () =>
      buildCancelGroups(candidates, search.data?.runs ?? {}, (pipeline) => ({
        name: getPluginDisplayName(plugins, pipeline.provider_type),
        canCancel: Boolean(getPluginByType(plugins, pipeline.provider_type)?.capabilities.cancel),
      })),
    [candidates, search.data, plugins]
  )
  const targets = cancellableGroups(groups)
  const where = (group: CancelGroup) =>
    actionTarget(group.pipeline, group.runs)
      .filter(({ label: field }) => field !== 'Pipeline' && field !== 'Branch')
      .map(({ label: field, value }) => `${field}: ${value}`)
      .join(' · ')
  const count = countRuns(targets)
  const label = scope?.label ?? ''
  const ready = !loading && count > 0

  const [state, formAction, isPending] = useActionState(
    async (): Promise<CancelActionState> => {
      try {
        await onConfirm(targets)
      } catch (error) {
        return { error: errorMessage(error, "Couldn't cancel the runs. Check them in your provider and try again.") }
      }

      onClose()

      return { error: null }
    },
    { error: null }
  )

  const status = loading
    ? `Looking for running runs… (${search.progress} of ${formatCount(candidates.length, 'pipeline')})`
    : `${formatCount(count, 'run')} can be cancelled.${unreadable.length > 0 ? ` Couldn't load runs for ${formatCount(unreadable.length, 'pipeline')}.` : ''}`

  const footer = ready
    ? (requestClose: () => void) => (
        <Group justify="flex-end" gap="sm" grow={isMobile || mobileShell}>
          <Button type="button" variant="default" size="sm" onClick={requestClose} disabled={isPending}>
            Keep running
          </Button>
          <Button type="submit" form={formId} color="red" size="sm" loading={isPending}>
            Cancel {formatCount(count, 'run')}
          </Button>
        </Group>
      )
    : undefined

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      size="md"
      title={ready ? `Cancel ${formatCount(count, 'run')} in ${label}` : `Cancel running in ${label}`}
      loading={isPending}
      footer={footer}
    >
      <LiveStatus message={status} />
      {loading ? (
        <LoadingState
          variant="compact"
          message={`Looking for running runs… (${search.progress} of ${formatCount(candidates.length, 'pipeline')})`}
        />
      ) : (
        <form id={formId} action={formAction}>
          <Stack gap={mobileShell ? 'lg' : 'md'}>
            <FormErrorDisplay title="Couldn't cancel runs" globalError={state.error} />
            {groups.length === 0 && unreadable.length === 0 && (
              <Text size="sm">
                {skippedNote ? `No runs to cancel in ${label}.` : `Nothing is running in ${label}.`}
              </Text>
            )}
            <CancelRunTargets groups={groups} now={now} shell={mobileShell} where={where} />
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
        </form>
      )}
    </StandardModal>
  )
}
