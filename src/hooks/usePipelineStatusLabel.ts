import { useNotificationStore } from '../stores/notificationStore'
import type { Pipeline } from '../types/pipelines'
import { getPipelineStatusLabel, isQuietWatched } from '../utils/pipelinePresentation'

type LabelledPipeline = Pick<Pipeline, 'id' | 'cadence' | 'overdue_run' | 'resource_state'>

export function usePipelineStatusLabel(): (pipeline: LabelledPipeline) => string | undefined {
  const quietPipelines = useNotificationStore((state) => state.preferences.quiet_pipelines)

  return (pipeline) => getPipelineStatusLabel(pipeline, isQuietWatched(pipeline, quietPipelines))
}
