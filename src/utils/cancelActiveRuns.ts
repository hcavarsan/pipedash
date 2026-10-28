import type { PaginatedRunHistory, Pipeline, PipelineRun, PipelineStatus } from '../types'

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

export interface UnfinishedRuns {
  runs: Record<string, PipelineRun[]>
  failed: Pipeline[]
}

export const findUnfinishedRuns = async (
  pipelines: Pipeline[],
  fetchPage: (pipelineId: string, page: number) => Promise<PaginatedRunHistory>,
  concurrency = 4,
  onProgress?: (done: number) => void
): Promise<UnfinishedRuns> => {
  const result: UnfinishedRuns = { runs: {}, failed: [] }
  const queue = [...pipelines]
  let done = 0

  const worker = async () => {
    for (let pipeline = queue.shift(); pipeline; pipeline = queue.shift()) {
      const found: PipelineRun[] = []

      try {
        for (let page = 1, more = true; more; page++) {
          const history = await fetchPage(pipeline.id, page)
          const unfinished = history.runs.filter((run) => isUnfinished(run.status))

          found.push(...unfinished)
          more = history.has_more && unfinished.length > 0
        }
      } catch {
        result.failed.push(pipeline)
      }

      if (found.length > 0) {
        result.runs[pipeline.id] = found
      }

      onProgress?.(++done)
    }
  }

  await Promise.all(Array.from({ length: Math.min(concurrency, queue.length) }, worker))

  return result
}

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
