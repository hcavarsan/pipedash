import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { GC_TIMES } from '../../../lib/cacheConfig'
import { notifications } from '../../../lib/notifications'
import { queryKeys, ROW_ACTION_KEY } from '../../../lib/queryKeys'
import { useRefreshProvider } from '../../../queries/useRefreshQueries'
import { service } from '../../../services/service'
import type { Pipeline, TriggerDraft, TriggerParams, TriggerResult } from '../../../types/pipelines'
import type { ActionCopy } from '../../../types/presentation'
import { errorMessage } from '../../../utils/errorDisplay'

export function usePreparedTrigger(
  pipelineId: string,
  rerunOf: number | undefined,
  enabled = true
): UseQueryResult<TriggerDraft, Error> {
  return useQuery({
    queryKey: [...queryKeys.workflows.all, 'trigger', pipelineId, rerunOf ?? null],
    queryFn: () => service.preparePipelineTrigger(pipelineId, rerunOf ?? null, null),
    staleTime: 0,
    gcTime: GC_TIMES.SHORT,
    enabled: enabled && !!pipelineId,
    retry: false,
  })
}

interface TriggerRequest {
  params: TriggerParams
  copy: ActionCopy
}

export function useTriggerWorkflow(): UseMutationResult<TriggerResult, Error, TriggerRequest, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ params }: TriggerRequest) => service.triggerPipeline(params),

    onSuccess: (_data, { params, copy }) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(params.workflow_id) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

      notifications.show({ title: copy.success.title, message: copy.success.message, color: 'green' })
    },

    onError: (error: Error, { copy }) => {
      notifications.show({
        title: copy.error_title,
        message: errorMessage(error, 'Check your provider connection and try again.'),
        color: 'red',
      })
    },
  })
}

export function useEnableWorkflow(): UseMutationResult<void, Error, Pipeline, unknown> {
  const { mutate: refreshProvider } = useRefreshProvider()

  return useMutation({
    mutationKey: [...ROW_ACTION_KEY, 'enable'],
    mutationFn: (pipeline: Pipeline) => service.enablePipeline(pipeline.id),

    onSuccess: (_data, pipeline) => {
      refreshProvider(pipeline.provider_id)

      notifications.show({
        title: 'Workflow enabled',
        message: `${pipeline.name} runs on its triggers again`,
        color: 'green',
      })
    },

    onError: (error: Error) => {
      notifications.show({
        title: "Couldn't enable workflow",
        message: errorMessage(error, 'Check your provider connection and try again.'),
        color: 'red',
      })
    },
  })
}
