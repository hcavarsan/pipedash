import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { GC_TIMES } from '../../../lib/cacheConfig'
import { queryKeys, ROW_ACTION_KEY } from '../../../lib/queryKeys'
import { useRefreshProvider } from '../../../queries/useRefreshQueries'
import { service } from '../../../services/service'
import type { Pipeline, TriggerDraft, TriggerParams, TriggerResult } from '../../../types/pipelines'
import type { ActionCopy } from '../../../types/presentation'
import { displayErrorNotification, displaySuccessNotification, errorMessage } from '../../../utils/errorDisplay'

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

interface CheckTriggerRequest {
  pipelineId: string
  rerunOf: number | null
  inputs: Record<string, unknown>
}

export function useCheckTrigger(): UseMutationResult<TriggerDraft, Error, CheckTriggerRequest, unknown> {
  return useMutation({
    mutationFn: ({ pipelineId, rerunOf, inputs }: CheckTriggerRequest) =>
      service.preparePipelineTrigger(pipelineId, rerunOf, inputs),
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

      displaySuccessNotification(copy.success.message, copy.success.title)
    },

    onError: (error: Error, { copy }) => {
      displayErrorNotification(errorMessage(error, 'Check your provider connection and try again.'), copy.error_title)
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

      displaySuccessNotification(`${pipeline.name} runs on its triggers again`, 'Workflow enabled')
    },

    onError: (error: Error) => {
      displayErrorNotification(
        errorMessage(error, 'Check your provider connection and try again.'),
        "Couldn't enable workflow"
      )
    },
  })
}
