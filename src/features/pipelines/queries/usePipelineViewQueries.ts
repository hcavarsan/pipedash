import {
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { logger } from '../../../lib/logger'
import { notifications } from '../../../lib/notifications'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type {
  CreateViewInput,
  PinKind,
  PinnedItem,
  PipelineNavigation,
  PipelineView,
  PipelineViewPage,
  PipelineViewQuery,
  ViewEditorOptions,
  ViewMember,
  ViewPatch,
  ViewPipeline,
  ViewSelectionEntry,
} from '../../../types/views'
import { errorMessage } from '../../../utils/errorDisplay'

const LEGACY_PINNED_KEY = 'pipedash-pinned-pipelines'
const LEGACY_HIDDEN_KEY = 'pipedash-hidden-pipelines'

function invalidateViews(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.views() })
  void queryClient.invalidateQueries({ queryKey: [...queryKeys.pipelines.all, 'viewMembers'] })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pins() })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.selections() })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.editorOptions() })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.navigations() })
}

function notifyError(title: string, error: Error, fallback: string) {
  notifications.show({ title, message: errorMessage(error, fallback), color: 'red' })
}

export function usePipelineViews(enabled = true): UseQueryResult<PipelineView[], Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.views(),
    queryFn: () => service.listPipelineViews(),
    staleTime: STALE_TIMES.FAST_CHANGING,
    enabled,
  })
}

export function usePipelineViewPage(
  query: PipelineViewQuery,
  enabled: boolean
): UseQueryResult<PipelineViewPage, Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.page(query),
    queryFn: () => service.getPipelineViewPage(query),
    placeholderData: (previous) => (previous?.view.id === query.view ? previous : undefined),
    staleTime: STALE_TIMES.FAST_CHANGING,
    gcTime: GC_TIMES.SHORT,
    enabled,
  })
}

export function useViewMembers(id: string, enabled: boolean): UseQueryResult<ViewMember[], Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.viewMembers(id),
    queryFn: () => service.listPipelineViewMembers(id),
    enabled,
  })
}

export function useViewSelection(pipelineIds: string[], enabled: boolean): UseQueryResult<ViewSelectionEntry[], Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.selection(pipelineIds),
    queryFn: () => service.getViewSelection(pipelineIds),
    enabled,
  })
}

export function useViewEditorOptions(view: string | null, enabled: boolean): UseQueryResult<ViewEditorOptions, Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.editorOption(view),
    queryFn: () => service.getViewEditorOptions(view),
    enabled,
  })
}

export function useSubViewAllowance(views: PipelineView[]): (id: string) => boolean {
  const custom = views.filter((view) => view.kind === 'custom')
  const results = useQueries({
    queries: custom.map((view) => ({
      queryKey: queryKeys.pipelines.editorOption(view.id),
      queryFn: () => service.getViewEditorOptions(view.id),
    })),
  })
  const allowed = new Set(custom.filter((_, index) => results[index]?.data?.can_add_sub_view).map((view) => view.id))

  return (id) => allowed.has(id)
}

export function usePipelineNavigation(search: string): UseQueryResult<PipelineNavigation, Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.navigation(search),
    queryFn: () => service.getPipelineNavigation(search || undefined),
    staleTime: STALE_TIMES.FAST_CHANGING,
    placeholderData: (previous) => previous,
  })
}

export function usePins(): UseQueryResult<PinnedItem[], Error> {
  return useQuery({
    queryKey: queryKeys.pipelines.pins(),
    queryFn: () => service.listPins(),
    staleTime: STALE_TIMES.FAST_CHANGING,
  })
}

export function useSetPin(): UseMutationResult<void, Error, { kind: PinKind; ref: string; pinned: boolean }, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ kind, ref, pinned }: { kind: PinKind; ref: string; pinned: boolean }) =>
      service.setPin(kind, ref, pinned),
    onError: (error: Error) => notifyError("Couldn't update pin", error, 'Try pinning it again.'),
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useCreatePipelineView(): UseMutationResult<PipelineView, Error, CreateViewInput, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: CreateViewInput) => service.createPipelineView(input),
    onSuccess: (view) => {
      invalidateViews(queryClient)
      notifications.show({ title: 'View created', message: view.name, color: 'green' })
    },
    onError: (error: Error) => notifyError("Couldn't create view", error, 'Try creating the view again.'),
  })
}

export function useUpdatePipelineView(): UseMutationResult<
  PipelineView,
  Error,
  { id: string; patch: ViewPatch },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ViewPatch }) => service.updatePipelineView(id, patch),
    onSuccess: (view, { patch }) => {
      if (patch.name !== undefined || patch.rules !== undefined || patch.members !== undefined) {
        notifications.show({ title: 'View saved', message: view.name, color: 'green' })
      }
    },
    onError: (error: Error) => notifyError("Couldn't save view", error, 'Try saving the view again.'),
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useDeletePipelineView(): UseMutationResult<void, Error, string, unknown> {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const currentView = searchParams.get('view')

  return useMutation({
    mutationFn: (id: string) => service.deletePipelineView(id),
    onSuccess: (_data, id) => {
      invalidateViews(queryClient)
      notifications.show({ title: 'View deleted', message: 'The view was removed.', color: 'green' })

      if (currentView === id) {
        void navigate('/pipelines', { replace: true })
      }
    },
    onError: (error: Error) => notifyError("Couldn't delete view", error, 'Try deleting the view again.'),
  })
}

export function useReorderPipelineViews(): UseMutationResult<void, Error, string[], unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (order: string[]) => service.reorderPipelineViews(order),
    onError: (error: Error) => notifyError("Couldn't reorder views", error, 'Try moving the view again.'),
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useSetViewMembership(): UseMutationResult<
  ViewSelectionEntry[],
  Error,
  { id: string; pipelineIds: string[]; member: boolean },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, pipelineIds, member }: { id: string; pipelineIds: string[]; member: boolean }) =>
      service.setViewMemberships(id, { pipeline_ids: pipelineIds, member }),
    onSettled: () => invalidateViews(queryClient),
    onError: (error: Error) => notifyError("Couldn't update view", error, 'Try updating the view again.'),
  })
}

export function useTogglePin(): UseMutationResult<void, Error, ViewPipeline, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipeline: ViewPipeline) => service.setPipelineViewMembership('pinned', pipeline.id, !pipeline.pinned),
    onError: (error: Error) => notifyError("Couldn't update pin", error, 'Try pinning the workflow again.'),
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useReorderPinned(): UseMutationResult<PipelineView, Error, string[], unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (members: string[]) => service.updatePipelineView('pinned', { members }),
    onError: (error: Error) => notifyError("Couldn't reorder pinned workflows", error, 'Try moving it again.'),
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useToggleHidden(): UseMutationResult<void, Error, ViewPipeline, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipeline: ViewPipeline) => service.setPipelineHidden(pipeline.id, !pipeline.hidden),
    onError: (error: Error) => notifyError("Couldn't update hidden pipelines", error, 'Try hiding the workflow again.'),
    onSettled: () => invalidateViews(queryClient),
  })
}

function readLegacyKeys(storageKey: string, field: string): string[] {
  try {
    const raw = localStorage.getItem(storageKey)
    const value = raw ? JSON.parse(raw)?.state?.[field] : undefined

    return Array.isArray(value) ? value.filter((key): key is string => typeof key === 'string') : []
  } catch {
    return []
  }
}

export function useLegacyPipelineMarksImport(enabled: boolean): void {
  const queryClient = useQueryClient()
  const started = useRef(false)

  useEffect(() => {
    if (!enabled || started.current) {
      return
    }

    started.current = true

    const pinned = readLegacyKeys(LEGACY_PINNED_KEY, 'pinnedKeys')
    const hidden = readLegacyKeys(LEGACY_HIDDEN_KEY, 'hiddenKeys')
    const clear = () => {
      localStorage.removeItem(LEGACY_PINNED_KEY)
      localStorage.removeItem(LEGACY_HIDDEN_KEY)
    }

    if (pinned.length === 0 && hidden.length === 0) {
      clear()

      return
    }

    service
      .importLegacyPipelineMarks({ pinned, hidden })
      .then(() => {
        clear()
        void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
        void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.views() })
      })
      .catch((error) => logger.warn('PipelineViews', 'Legacy pin/hide import failed', error))
  }, [enabled, queryClient])
}
