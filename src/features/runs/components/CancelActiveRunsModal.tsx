import { Button, Group, Stack, Text } from '@mantine/core'
import { type ReactNode, useActionState, useId, useMemo } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import type { Pipeline } from '../../../types/pipelines'
import type { CancelGroup, CancellationTargets } from '../../../types/run_action_batch'
import { formatCount } from '../../../utils/formatCount'
import { useCancellationTargets } from '../queries/useCancellationTargets'

import { CancelRunTargets } from './CancelRunTargets'

export interface CancelScope {
  label: string
  pipelines: Pipeline[]
}

interface CancelActiveRunsModalProps {
  opened: boolean
  scope: CancelScope | null
  onClose: () => void
  onConfirm: (targets: CancellationTargets) => Promise<unknown>
}

const where = ({ pipeline }: CancelGroup): string =>
  pipeline.actions.target ? `${pipeline.actions.target.label}: ${pipeline.actions.target.value}` : ''

const CancelNote = ({ color, text }: { color: string; text: string | null }): ReactNode =>
  text ? (
    <Text size="xs" c={color}>
      {text}
    </Text>
  ) : null

export const CancelActiveRunsModal = ({ opened, scope, onClose, onConfirm }: CancelActiveRunsModalProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const formId = useId()
  const pipelineIds = useMemo(() => scope?.pipelines.map((pipeline) => pipeline.id) ?? [], [scope])
  const search = useCancellationTargets(pipelineIds, opened)
  const failed = search.isError
  const loading = search.isFetching && !failed
  const groups = search.data?.groups ?? []
  const unreadable = search.data?.failed_pipeline_ids ?? []
  const skippedNote = (search.data?.skipped ?? []).map((entry) => entry.reason).join(' ')
  const count = search.data?.run_count ?? 0
  const label = scope?.label ?? ''
  const emptyNote =
    groups.length === 0 && unreadable.length === 0
      ? `${skippedNote ? 'No runs to cancel in' : 'Nothing is running in'} ${label}.`
      : null
  const unreadableNote =
    unreadable.length > 0 ? `Couldn't load runs for ${formatCount(unreadable.length, 'pipeline')}.` : null
  const ready = !loading && !failed && count > 0

  const [, formAction, isPending] = useActionState(async (): Promise<null> => {
    try {
      if (!search.data) {
        return null
      }
      await onConfirm(search.data)
    } catch {
      return null
    }

    onClose()

    return null
  }, null)

  const found = `${formatCount(count, 'run')} can be cancelled.${unreadable.length > 0 ? ` Couldn't load runs for ${formatCount(unreadable.length, 'pipeline')}.` : ''}`
  const progress = loading ? 'Looking for running runs…' : found
  const status = failed ? "Couldn't find running runs." : progress

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
      {failed && (
        <ErrorState
          title="Couldn't find running runs"
          error={search.error}
          onRetry={() => void search.refetch()}
          retrying={search.isFetching}
        />
      )}
      {loading && <LoadingState variant="compact" message="Looking for running runs…" />}
      {!failed && !loading && (
        <form id={formId} action={formAction}>
          <Stack gap={mobileShell ? 'lg' : 'md'}>
            {emptyNote && <Text size="sm">{emptyNote}</Text>}
            <CancelRunTargets groups={groups} now={now} shell={mobileShell} where={where} />
            <CancelNote color="red" text={unreadableNote} />
            <CancelNote color="orange" text={skippedNote} />
          </Stack>
        </form>
      )}
    </StandardModal>
  )
}
