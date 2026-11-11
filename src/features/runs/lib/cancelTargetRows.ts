import type { Pipeline, PipelineRun } from '../../../types/pipelines'
import type { DisplayField } from '../../../types/presentation'

export const cancelTargetRows = (pipeline: Pipeline, runs: Pick<PipelineRun, 'branch'>[]): DisplayField[] => {
  const branches = [...new Set((runs.length > 0 ? runs.map((run) => run.branch) : [pipeline.branch]).filter(Boolean))]

  return [
    { label: 'Pipeline', value: pipeline.name },
    { label: 'Repository', value: pipeline.repository },
    { label: 'Branch', value: branches.join(', ') },
    ...(pipeline.actions.target ? [pipeline.actions.target] : []),
  ].filter(({ value }) => value)
}
