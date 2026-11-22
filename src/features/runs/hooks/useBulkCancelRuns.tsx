import { useCallback, useMemo, useState } from 'react'
import { confirmBody } from '../../../components/common/ActionTargetRows'
import type { Pipeline, PipelineRun } from '../../../types/pipelines'
import { openConfirm } from '../../../utils/confirm'
import { cancelTargetRows } from '../lib/cancelTargetRows'
import { useCancelRuns } from '../queries/useCancelRuns'

interface UseBulkCancelRunsOptions {
  pipeline: Pipeline | undefined
  page: number
  runs: PipelineRun[]
  enabled: boolean
  cancellingRunNumber: number | null
}

interface UseBulkCancelRunsResult {
  selectedRuns: PipelineRun[]
  isRunCancellable: (run: PipelineRun) => boolean
  selectRuns: (records: PipelineRun[]) => void
  cancelSelected: () => void
  isCancelling: boolean
}

export function useBulkCancelRuns({
  pipeline,
  page,
  runs,
  enabled,
  cancellingRunNumber,
}: UseBulkCancelRunsOptions): UseBulkCancelRunsResult {
  const pipelineId = pipeline?.id
  const [selectedRunNumbers, setSelectedRunNumbers] = useState<number[]>([])
  const cancelRunsMutation = useCancelRuns()

  const isRunCancellable = useCallback(
    (run: PipelineRun) => enabled && run.actions.cancel.allowed && cancellingRunNumber !== run.run_number,
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
    if (!pipeline || !pipelineId || selectedRuns.length === 0) {
      return
    }

    const runNumbers = selectedRuns.map((run) => run.run_number)
    const cancelLabel = runNumbers.length === 1 ? 'Cancel run' : `Cancel ${runNumbers.length} runs`

    openConfirm({
      title: cancelLabel,
      children: confirmBody(
        `This cancels ${runNumbers.length === 1 ? 'run' : 'runs'} ${runNumbers.map((runNumber) => `#${runNumber}`).join(', ')}. You can't undo this.`,
        cancelTargetRows(pipeline, selectedRuns)
      ),
      labels: { confirm: cancelLabel, cancel: 'Keep running' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        try {
          await cancelRunsMutation.mutateAsync([
            runNumbers.map((runNumber) => ({ pipeline_id: pipelineId, run_number: runNumber })),
          ])
        } catch {
          return
        }

        setSelectedRunNumbers([])
      },
    })
  }, [pipeline, pipelineId, selectedRuns, cancelRunsMutation])

  return {
    selectedRuns,
    isRunCancellable,
    selectRuns,
    cancelSelected,
    isCancelling: cancelRunsMutation.isPending,
  }
}
