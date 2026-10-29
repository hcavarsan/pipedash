import { notifications } from '@mantine/notifications'
import {
  type QueryClient,
  type QueryKey,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'

import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type {
  CreateViewInput,
  PinKind,
  PinnedItem,
  PipelineView,
  PipelineViewPage,
  PipelineViewQuery,
  ViewMember,
  ViewPatch,
  ViewPipeline,
} from '../../../types/domain'
import { getErrorCode } from '../../../types/errors'
import { errorMessage } from '../../../utils/errorDisplay'

const LEGACY_PINNED_KEY = 'pipedash-pinned-pipelines'
const LEGACY_HIDDEN_KEY = 'pipedash-hidden-pipelines'

function invalidateViews(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.views() })
  void queryClient.invalidateQueries({ queryKey: [...queryKeys.pipelines.all, 'viewMembers'] })
  void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pins() })
}

function notifyError(title: string, error: Error, fallback: string) {
  notifications.show({ title, message: errorMessage(error, fallback), color: 'red' })
}

function updatePages(queryClient: QueryClient, update: (page: PipelineViewPage) => PipelineViewPage) {
  queryClient.setQueriesData<PipelineViewPage>({ queryKey: queryKeys.pipelines.pages() }, (page) =>
    page ? update(page) : page
  )
}

function updatePagePipeline(queryClient: QueryClient, id: string, patch: Partial<ViewPipeline>) {
  const apply = (pipelines: ViewPipeline[]) => pipelines.map((p) => (p.id === id ? { ...p, ...patch } : p))

  updatePages(queryClient, (page) => ({
    ...page,
    pinned: apply(page.pinned),
    sections: page.sections.map((section) => ({ ...section, pipelines: apply(section.pipelines) })),
  }))
}

type PageSnapshots = Array<[QueryKey, PipelineViewPage | undefined]>

interface DisplaySnapshots {
  pages: PageSnapshots
  views: PipelineView[] | undefined
}

function snapshotPages(queryClient: QueryClient): PageSnapshots {
  return queryClient.getQueriesData<PipelineViewPage>({ queryKey: queryKeys.pipelines.pages() })
}

function restorePages(queryClient: QueryClient, snapshots: PageSnapshots | undefined) {
  for (const [key, page] of snapshots ?? []) {
    queryClient.setQueryData(key, page)
  }
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
    retry: (failureCount, error) => getErrorCode(error) !== 'NOT_FOUND' && failureCount < 3,
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
  DisplaySnapshots | undefined
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ViewPatch }) => service.updatePipelineView(id, patch),
    onMutate: async ({ id, patch }) => {
      const { display } = patch

      if (!display || patch.name !== undefined || patch.rules !== undefined || patch.members !== undefined) {
        return undefined
      }

      const setDisplay = (view: PipelineView): PipelineView => (view.id === id ? { ...view, display } : view)

      await queryClient.cancelQueries({ queryKey: queryKeys.pipelines.pages() })
      const snapshots: DisplaySnapshots = {
        pages: snapshotPages(queryClient),
        views: queryClient.getQueryData<PipelineView[]>(queryKeys.pipelines.views()),
      }
      updatePages(queryClient, (page) => ({ ...page, view: setDisplay(page.view) }))
      queryClient.setQueryData<PipelineView[]>(queryKeys.pipelines.views(), (views) => views?.map(setDisplay))

      return snapshots
    },
    onSuccess: (view, { patch }) => {
      if (patch.name !== undefined || patch.rules !== undefined || patch.members !== undefined) {
        notifications.show({ title: 'View saved', message: view.name, color: 'green' })
      }
    },
    onError: (error: Error, _variables, snapshots) => {
      if (snapshots) {
        restorePages(queryClient, snapshots.pages)
        queryClient.setQueryData(queryKeys.pipelines.views(), snapshots.views)
      }

      notifyError("Couldn't save view", error, 'Try saving the view again.')
    },
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

export function useReorderPipelineViews(): UseMutationResult<
  void,
  Error,
  string[],
  { previous: PipelineView[] | undefined }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (order: string[]) => service.reorderPipelineViews(order),
    onMutate: async (order) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.pipelines.views() })

      const previous = queryClient.getQueryData<PipelineView[]>(queryKeys.pipelines.views())

      queryClient.setQueryData<PipelineView[]>(queryKeys.pipelines.views(), (views) => {
        if (!views) {
          return views
        }

        const movable = order
          .map((id) => views.find((view) => view.id === id))
          .filter((view): view is PipelineView => view !== undefined)
        let next = 0

        return views.map((view) => (order.includes(view.id) ? (movable[next++] ?? view) : view))
      })

      return { previous }
    },
    onError: (error: Error, _order, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKeys.pipelines.views(), context.previous)
      }

      notifyError("Couldn't reorder views", error, 'Try moving the view again.')
    },
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useSetViewMembership(): UseMutationResult<
  void,
  Error,
  { id: string; pipelineIds: string[]; member: boolean },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ id, pipelineIds, member }: { id: string; pipelineIds: string[]; member: boolean }) => {
      for (const pipelineId of pipelineIds) {
        await service.setPipelineViewMembership(id, pipelineId, member)
      }
    },
    onSettled: () => invalidateViews(queryClient),
    onError: (error: Error) => notifyError("Couldn't update view", error, 'Try updating the view again.'),
  })
}

export function useTogglePin(): UseMutationResult<void, Error, ViewPipeline, PageSnapshots> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipeline: ViewPipeline) => service.setPipelineViewMembership('pinned', pipeline.id, !pipeline.pinned),
    onMutate: async (pipeline) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.pipelines.pages() })
      const snapshots = snapshotPages(queryClient)
      updatePagePipeline(queryClient, pipeline.id, { pinned: !pipeline.pinned })

      return snapshots
    },
    onError: (error: Error, _pipeline, snapshots) => {
      restorePages(queryClient, snapshots)
      notifyError("Couldn't update pin", error, 'Try pinning the workflow again.')
    },
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useToggleHidden(): UseMutationResult<void, Error, ViewPipeline, PageSnapshots> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (pipeline: ViewPipeline) => service.setPipelineHidden(pipeline.id, !pipeline.hidden),
    onMutate: async (pipeline) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.pipelines.pages() })
      const snapshots = snapshotPages(queryClient)
      updatePagePipeline(queryClient, pipeline.id, { hidden: !pipeline.hidden })

      return snapshots
    },
    onError: (error: Error, _pipeline, snapshots) => {
      restorePages(queryClient, snapshots)
      notifyError("Couldn't update hidden pipelines", error, 'Try hiding the workflow again.')
    },
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
