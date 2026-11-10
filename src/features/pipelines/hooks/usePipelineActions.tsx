import type { ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import { usePlugins } from '../../../queries/usePluginsQueries'
import { useRefreshProviders } from '../../../queries/useRefreshQueries'
import type { PipelineActions } from '../../../types/pipelineRows'
import type { Pipeline } from '../../../types/pipelines'
import type { PipelineView, ViewDisplay, ViewPipeline } from '../../../types/views'
import { usePipelineNotifyItem } from '../../notifications/components/PipelineNotifyPolicy'
import { useEnableWorkflow } from '../../runs/queries/useWorkflowQueries'
import {
  type PipelineMark,
  useBusyPipelines,
  useSetPipelineMarks,
  useSetViewMembership,
  useUpdatePipelineView,
} from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { useCancelActiveRuns } from './useCancelActiveRuns'
import { usePinning } from './usePinning'

interface PipelineActionsInput {
  view: PipelineView | undefined
  onTrigger: (pipeline: Pipeline) => void
  onViewHistory: (pipeline: Pipeline) => void
  onViewMetrics?: ((pipeline: Pipeline) => void) | undefined
}

interface PipelineActionsResult {
  actions: PipelineActions
  changeDisplay: (display: ViewDisplay) => void
  cancelDialog: ReactNode
  viewUpdating: boolean
}

export function usePipelineActions({
  view,
  onTrigger,
  onViewHistory,
  onViewMetrics,
}: PipelineActionsInput): PipelineActionsResult {
  const { mutate: setMarks } = useSetPipelineMarks()
  const { mutate: setMembership } = useSetViewMembership()
  const { mutate: updateView, isPending: updatingView } = useUpdatePipelineView()
  const { refresh: refreshProviders, refreshing } = useRefreshProviders()
  const { mutate: enableWorkflow } = useEnableWorkflow()
  const { isBusy, isBusyNow } = useBusyPipelines()
  const { data: plugins } = usePlugins()
  const pinItem = usePinning()
  const notifyItem = usePipelineNotifyItem()
  const { cancelItem, dialog: cancelDialog } = useCancelActiveRuns()
  const [, setSearchParams] = useSearchParams()
  const canRemove = view?.kind === 'custom' && view.capabilities.edit_members

  const openView = (id: string) =>
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev)

        next.set('view', id)

        return next
      },
      { replace: true }
    )

  const setMarksOf = (mark: PipelineMark, pipelines: ViewPipeline[], value: boolean, onSuccess?: () => void) => {
    const pipelineIds = pipelines.map((p) => p.id).filter((id) => !isBusyNow(id))

    if (pipelineIds.length > 0) {
      setMarks({ mark, pipelineIds, value }, onSuccess ? { onSuccess } : undefined)
    }
  }
  const whenFree = (pipeline: Pipeline, run: () => void) => {
    if (!isBusyNow(pipeline.id)) {
      run()
    }
  }

  const actions: PipelineActions = {
    onTogglePin: (p) => setMarksOf('pinned', [p], !p.pinned),
    onToggleHidden: (p) => setMarksOf('hidden', [p], !p.hidden),
    onSetMarks: setMarksOf,
    isBusy,
    onAddToView: (p) => usePipelineViewStore.getState().openAddToView([p.id]),
    onRemoveFromView:
      view && canRemove
        ? (p) => whenFree(p, () => setMembership({ id: view.id, pipelineIds: [p.id], member: false }))
        : undefined,
    removeLabel: view && canRemove ? `Remove from ${view.name}` : undefined,
    onTrigger,
    onEnable: (p) => whenFree(p, () => enableWorkflow(p)),
    onViewMetrics,
    onViewHistory,
    onOpenView: openView,
    onRefreshProviders: (ids) => void refreshProviders(ids),
    isRefreshing: (ids) => ids.some((id) => refreshing.has(id)),
    pinItem,
    cancelItem,
    notifyItem,
    terminology: (providerType) =>
      plugins?.find((plugin) => plugin.provider_type === providerType)?.presentation.terminology,
  }

  const changeDisplay = (next: ViewDisplay) => {
    if (view && !updatingView) {
      updateView({ id: view.id, patch: { display: next } })
    }
  }

  return { actions, changeDisplay, cancelDialog, viewUpdating: updatingView }
}
