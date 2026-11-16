import {
  type InfiniteData,
  type QueryClient,
  type UseInfiniteQueryResult,
  type UseMutationResult,
  type UseQueryResult,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type {
  ApplicationDetail,
  ApplicationEdit,
  ApplicationList,
  ApplicationListQuery,
  ApplicationMetrics,
  ApplicationMetricsQuery,
  ApplicationRef,
  ChangesPage,
  CommitLocations,
  CreateApplicationInput,
  DeployChanges,
  Suggestion,
} from '../../../types/applications'
import { displayErrorNotification } from '../../../utils/errorDisplay'

const CHANGES_PAGE_SIZE = 50
const FIRST_CHANGES_PAGE: string | null = null

export async function invalidateApplications(queryClient: QueryClient): Promise<void> {
  await queryClient.invalidateQueries({ queryKey: queryKeys.applicationsAll })
}

export function useApplications(query: ApplicationListQuery): UseQueryResult<ApplicationList, Error> {
  return useQuery({
    queryKey: queryKeys.applications(query),
    queryFn: () => service.listApplications(query),
    placeholderData: (previous) => previous,
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
  })
}

export function useApplication(id: number): UseQueryResult<ApplicationDetail, Error> {
  return useQuery({
    queryKey: queryKeys.application(id),
    queryFn: () => service.getApplication(id),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useApplicationChanges(
  id: number
): UseInfiniteQueryResult<InfiniteData<ChangesPage, string | null>, Error> {
  return useInfiniteQuery({
    queryKey: queryKeys.applicationChanges(id),
    queryFn: ({ pageParam }) => service.getApplicationChanges(id, pageParam, CHANGES_PAGE_SIZE),
    initialPageParam: FIRST_CHANGES_PAGE,
    getNextPageParam: (lastPage) => (lastPage.has_more ? lastPage.next_cursor : undefined),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.MEDIUM,
  })
}

export function useApplicationMetrics(query: ApplicationMetricsQuery): UseQueryResult<ApplicationMetrics, Error> {
  return useQuery({
    queryKey: queryKeys.applicationMetrics(query),
    queryFn: () => service.getApplicationMetrics(query),
    placeholderData: (previous) => previous,
    staleTime: STALE_TIMES.MODERATE,
    gcTime: GC_TIMES.SHORT,
  })
}

export function useApplicationSuggestions(): UseQueryResult<Suggestion[], Error> {
  return useQuery({
    queryKey: queryKeys.applicationSuggestions(),
    queryFn: () => service.listApplicationSuggestions(),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function usePipelineApplications(pipelineId: string): UseQueryResult<ApplicationRef[], Error> {
  return useQuery({
    queryKey: queryKeys.pipelineApplications(pipelineId),
    queryFn: () => service.getPipelineApplications(pipelineId),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useCommitLocations(pipelineId: string, runNumber: number): UseQueryResult<CommitLocations, Error> {
  return useQuery({
    queryKey: queryKeys.commitLocations(pipelineId, runNumber),
    queryFn: () => service.getCommitLocations(pipelineId, runNumber),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useDeployChanges(pipelineId: string, runNumber: number): UseQueryResult<DeployChanges, Error> {
  return useQuery({
    queryKey: queryKeys.deployChanges(pipelineId, runNumber),
    queryFn: () => service.getDeployChanges(pipelineId, runNumber),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useCreateApplication(): UseMutationResult<ApplicationDetail, Error, CreateApplicationInput, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: CreateApplicationInput) => service.createApplication(input),
    onSuccess: () => invalidateApplications(queryClient),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't create application"),
  })
}

export function useEditApplication(): UseMutationResult<
  ApplicationDetail,
  Error,
  { id: number; edit: ApplicationEdit },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, edit }: { id: number; edit: ApplicationEdit }) => service.editApplication(id, edit),
    onSuccess: () => invalidateApplications(queryClient),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't update application"),
  })
}

export function useDeleteApplication(): UseMutationResult<void, Error, number, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: number) => service.deleteApplication(id),
    onSuccess: () => invalidateApplications(queryClient),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't delete application"),
  })
}

export function useRevertOverride(): UseMutationResult<
  ApplicationDetail,
  Error,
  { id: number; overrideId: number },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, overrideId }: { id: number; overrideId: number }) =>
      service.revertApplicationOverride(id, overrideId),
    onSuccess: () => invalidateApplications(queryClient),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't revert change"),
  })
}

export function useAcceptSuggestions(): UseMutationResult<void, Error, string[], unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (ids: string[]) => service.acceptApplicationSuggestions(ids),
    onSuccess: () => invalidateApplications(queryClient),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't accept suggestions"),
  })
}
