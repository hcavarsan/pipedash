import {
  type MutationKey,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useMutationState,
  useQueries,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { logger } from '../../../lib/logger'
import { queryKeys, ROW_ACTION_KEY } from '../../../lib/queryKeys'
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
  ViewSelectionEntry,
} from '../../../types/views'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'
import { isRecord } from '../../../utils/isRecord'

const LEGACY_PINNED_KEY = 'pipedash-pinned-pipelines'
const LEGACY_HIDDEN_KEY = 'pipedash-hidden-pipelines'
const PIN_KEY = [...ROW_ACTION_KEY, 'pin'] as const

export type PipelineMark = 'pinned' | 'hidden'

export interface MarkChange {
  mark: PipelineMark
  pipelineIds: string[]
  value: boolean
}

interface Busy {
  isBusy: (key: string) => boolean
  isBusyNow: (key: string) => boolean
}

export async function invalidateViews(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pages() }),
    queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.views() }),
    queryClient.invalidateQueries({ queryKey: [...queryKeys.pipelines.all, 'viewMembers'] }),
    queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pins() }),
    queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.selections() }),
    queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.editorOptions() }),
    queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.navigations() }),
  ])
}

function pipelineIdsOf(variables: unknown): string[] {
  if (!isRecord(variables)) {
    return []
  }

  const { pipelineIds, id } = variables

  if (Array.isArray(pipelineIds)) {
    return pipelineIds.filter((value): value is string => typeof value === 'string')
  }

  return typeof id === 'string' ? [id] : []
}

export const pinKey = (kind: string, ref: string): string => `${kind}:${ref}`

function pinKeysOf(variables: unknown): string[] {
  if (!isRecord(variables)) {
    return []
  }

  const { kind, ref } = variables

  return typeof kind === 'string' && typeof ref === 'string' ? [pinKey(kind, ref)] : []
}

function useBusy(mutationKey: MutationKey, keysOf: (variables: unknown) => string[]): Busy {
  const queryClient = useQueryClient()
  const pending = new Set(
    useMutationState({
      filters: { mutationKey, status: 'pending' },
      select: (mutation) => keysOf(mutation.state.variables),
    }).flat()
  )

  return {
    isBusy: (key) => pending.has(key),
    isBusyNow: (key) =>
      queryClient.isMutating({
        mutationKey,
        predicate: (mutation) => keysOf(mutation.state.variables).includes(key),
      }) > 0,
  }
}

export const useBusyPipelines = (): Busy => useBusy(ROW_ACTION_KEY, pipelineIdsOf)

export const useBusyPins = (): Busy => useBusy(PIN_KEY, pinKeysOf)

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
    mutationKey: PIN_KEY,
    mutationFn: ({ kind, ref, pinned }: { kind: PinKind; ref: string; pinned: boolean }) =>
      service.setPin(kind, ref, pinned),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't update pin"),
    onSettled: () => invalidateViews(queryClient),
  })
}

export function useCreatePipelineView(): UseMutationResult<PipelineView, Error, CreateViewInput, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: CreateViewInput) => service.createPipelineView(input),
    onSuccess: (view) => {
      void invalidateViews(queryClient)
      displaySuccessNotification(view.name, 'View created')
    },
    onError: (error: Error) => displayErrorNotification(error, "Couldn't create view"),
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
        displaySuccessNotification(view.name, 'View saved')
      }
    },
    onError: (error: Error) => displayErrorNotification(error, "Couldn't save view"),
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
      void invalidateViews(queryClient)
      displaySuccessNotification('The view was removed.', 'View deleted')

      if (currentView === id) {
        void navigate('/pipelines', { replace: true })
      }
    },
    onError: (error: Error) => displayErrorNotification(error, "Couldn't delete view"),
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
    mutationKey: [...ROW_ACTION_KEY, 'membership'],
    mutationFn: ({ id, pipelineIds, member }: { id: string; pipelineIds: string[]; member: boolean }) =>
      service.setViewMemberships(id, { pipeline_ids: pipelineIds, member }),
    onSuccess: (entries, { pipelineIds }) => {
      queryClient.setQueryData(queryKeys.pipelines.selection(pipelineIds), entries)
    },
    onSettled: () => invalidateViews(queryClient),
    onError: (error: Error) => displayErrorNotification(error, "Couldn't update view"),
  })
}

export function useSetPipelineMarks(): UseMutationResult<void, Error, MarkChange, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: [...ROW_ACTION_KEY, 'marks'],
    mutationFn: async ({ mark, pipelineIds, value }: MarkChange) => {
      const results = await Promise.allSettled(
        pipelineIds.map((id) =>
          mark === 'pinned'
            ? service.setPipelineViewMembership('pinned', id, value)
            : service.setPipelineHidden(id, value)
        )
      )
      const failed = results.find((result): result is PromiseRejectedResult => result.status === 'rejected')

      if (failed) {
        throw failed.reason
      }
    },
    onError: (error: Error, { mark }) =>
      displayErrorNotification(error, mark === 'pinned' ? "Couldn't update pin" : "Couldn't update hidden pipelines"),
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
