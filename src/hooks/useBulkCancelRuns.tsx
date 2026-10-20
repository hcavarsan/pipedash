import { Text } from '@mantine/core'
import { modals } from '@mantine/modals'
import { useCallback, useMemo, useState } from 'react'

import { useCancelRuns } from '../queries/useRunHistoryQueries'
import type { PipelineRun } from '../types'

interface UseBulkCancelRunsOptions {
  pipelineId: string | undefined
  page: number
  runs: PipelineRun[]
  enabled: boolean
  cancellingRunNumber: number | null
}

export function useBulkCancelRuns({ pipelineId, page, runs, enabled, cancellingRunNumber }: UseBulkCancelRunsOptions) {
  const [selectedRunNumbers, setSelectedRunNumbers] = useState<number[]>([])
  const cancelRunsMutation = useCancelRuns()

  const isRunCancellable = useCallback(
    (run: PipelineRun) => enabled && run.status === 'running' && cancellingRunNumber !== run.run_number,
    [enabled, cancellingRunNumber]
  )

  const selectedRuns = useMemo(
    () => runs.filter((run) => selectedRunNumbers.includes(run.run_number) && isRunCancellable(run)),
    [runs, selectedRunNumbers, isRunCancellable]
  )

  const [scope, setScope] = useState({ pipelineId, page })

  if (scope.pipelineId !== pipelineId || scope.page !== page) {
    setScope({ pipelineId, page })
    setSelectedRunNumbers([])
  }

  const selectRuns = useCallback((records: PipelineRun[]) => {
    setSelectedRunNumbers(records.map((run) => run.run_number))
  }, [])

  const cancelSelected = useCallback(() => {
    if (!pipelineId || selectedRuns.length === 0) {
      return
    }

    const runNumbers = selectedRuns.map((run) => run.run_number)

    modals.openConfirmModal({
      title: 'Cancel selected runs',
      children: (
        <Text size="sm">
          This cancels {runNumbers.length} {runNumbers.length === 1 ? 'run' : 'runs'}:{' '}
          {runNumbers.map((runNumber) => `#${runNumber}`).join(', ')}. You can't undo this.
        </Text>
      ),
      labels: { confirm: 'Cancel runs', cancel: 'Keep running' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        await cancelRunsMutation.mutateAsync({ pipelineId, runNumbers })
        setSelectedRunNumbers([])
      },
    })
  }, [pipelineId, selectedRuns, cancelRunsMutation])

  return {
    selectedRuns,
    isRunCancellable,
    selectRuns,
    cancelSelected,
    isCancelling: cancelRunsMutation.isPending,
  }
}
