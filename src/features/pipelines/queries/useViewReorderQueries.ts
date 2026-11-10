import {
  type QueryClient,
  type QueryKey,
  type UseMutationResult,
  useIsMutating,
  useMutation,
  useQueryClient,
} from '@tanstack/react-query'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { PipelineView, ViewMember } from '../../../types/views'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { invalidateViews } from './usePipelineViewQueries'

const VIEW_REORDER_KEY = ['pipelineViewReorder'] as const

interface OrderSnapshot<T> {
  queryKey: QueryKey
  previous: T[] | undefined
}

async function holdOrder<T>(
  queryClient: QueryClient,
  queryKey: QueryKey,
  idOf: (item: T) => string,
  order: string[]
): Promise<OrderSnapshot<T>> {
  await queryClient.cancelQueries({ queryKey })

  const previous = queryClient.getQueryData<T[]>(queryKey)
  const rank = (item: T) => {
    const index = order.indexOf(idOf(item))

    return index < 0 ? order.length : index
  }

  if (previous) {
    queryClient.setQueryData(
      queryKey,
      [...previous].sort((a, b) => rank(a) - rank(b))
    )
  }

  return { queryKey, previous }
}

function releaseOrder<T>(queryClient: QueryClient, snapshot: OrderSnapshot<T> | undefined): void {
  if (snapshot?.previous) {
    queryClient.setQueryData(snapshot.queryKey, snapshot.previous)
  }
}

export function useReorderPipelineViews(): UseMutationResult<void, Error, string[], OrderSnapshot<PipelineView>> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: VIEW_REORDER_KEY,
    mutationFn: (order: string[]) => service.reorderPipelineViews(order),
    onMutate: (order) => holdOrder(queryClient, queryKeys.pipelines.views(), (view: PipelineView) => view.id, order),
    onError: (error: Error, _order, snapshot) => {
      releaseOrder(queryClient, snapshot)
      displayErrorNotification(error, "Couldn't reorder views")
    },
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useViewsReordering(): { reordering: boolean; reorderingNow: () => boolean } {
  const queryClient = useQueryClient()
  const reordering = useIsMutating({ mutationKey: VIEW_REORDER_KEY }) > 0

  return { reordering, reorderingNow: () => queryClient.isMutating({ mutationKey: VIEW_REORDER_KEY }) > 0 }
}

export function useReorderPinned(): UseMutationResult<PipelineView, Error, string[], OrderSnapshot<ViewMember>> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (members: string[]) => service.updatePipelineView('pinned', { members }),
    onMutate: (members) =>
      holdOrder(
        queryClient,
        queryKeys.pipelines.viewMembers('pinned'),
        (member: ViewMember) => member.pipeline_id,
        members
      ),
    onError: (error: Error, _members, snapshot) => {
      releaseOrder(queryClient, snapshot)
      displayErrorNotification(error, "Couldn't reorder pinned workflows")
    },
    onSettled: () => invalidateViews(queryClient),
  })
}
