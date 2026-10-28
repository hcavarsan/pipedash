import type { Pipeline, PipelineRun, PipelineStatus } from '../types'

import { formatCount } from './formatCount'

export const isUnfinished = (status: PipelineStatus) => status === 'running' || status === 'pending'

export const unfinishedPipelines = <T extends Pick<Pipeline, 'status'>>(pipelines: T[]) =>
  pipelines.filter((pipeline) => isUnfinished(pipeline.status))

export interface CancelProvider {
  name: string
  canCancel: boolean
}

export interface CancelGroup extends CancelProvider {
  pipeline: Pipeline
  runs: PipelineRun[]
}

export interface CancelResult {
  pipelineId: string
  pipelineName: string
  runNumber: number
  error: string | null
}

export const buildCancelGroups = (
  pipelines: Pipeline[],
  runsByPipeline: Record<string, PipelineRun[]>,
  providerOf: (pipeline: Pipeline) => CancelProvider
): CancelGroup[] =>
  pipelines.flatMap((pipeline) => {
    const runs = (runsByPipeline[pipeline.id] ?? []).filter((run) => isUnfinished(run.status))

    return runs.length > 0 ? [{ pipeline, runs, ...providerOf(pipeline) }] : []
  })

export const cancellableGroups = (groups: CancelGroup[]) => groups.filter((group) => group.canCancel)

export const countRuns = (groups: CancelGroup[]) => groups.reduce((sum, group) => sum + group.runs.length, 0)

export const summarizeCancellation = (results: CancelResult[]) => {
  const failed = results.filter((result) => result.error !== null)
  const cancelled = results.length - failed.length

  if (failed.length === 0) {
    return {
      title: 'Cancellation requested',
      message: `Cancellation requested for ${formatCount(cancelled, 'run')}`,
      color: 'gray',
    }
  }

  return {
    title: "Couldn't cancel every run",
    message: `Cancellation requested for ${cancelled} of ${formatCount(results.length, 'run')}. Failed: ${failed
      .map((result) => `${result.pipelineName} #${result.runNumber} (${result.error})`)
      .join('; ')}`,
    color: 'red',
  }
}
