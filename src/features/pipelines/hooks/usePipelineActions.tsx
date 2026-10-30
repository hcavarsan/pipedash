import type { ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { PipelineActions } from '../../../components/common/PipelineRowParts'
import { useRefreshProviders } from '../../../queries/useRefreshQueries'
import type { Pipeline, PipelineView, ViewDisplay } from '../../../types/domain'
import { useEnableWorkflow } from '../../runs/queries/useWorkflowQueries'
import {
  useSetViewMembership,
  useToggleHidden,
  useTogglePin,
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
}

export function usePipelineActions({
  view,
  onTrigger,
  onViewHistory,
  onViewMetrics,
}: PipelineActionsInput): PipelineActionsResult {
  const { mutate: togglePin } = useTogglePin()
  const { mutate: toggleHidden } = useToggleHidden()
  const { mutate: setMembership } = useSetViewMembership()
  const { mutate: updateView } = useUpdatePipelineView()
  const { refresh: refreshProviders, refreshing } = useRefreshProviders()
  const { mutate: enableWorkflow } = useEnableWorkflow()
  const pinItem = usePinning()
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

  const actions: PipelineActions = {
    onTogglePin: (p) => togglePin(p),
    onToggleHidden: (p) => toggleHidden(p),
    onAddToView: (p) => usePipelineViewStore.getState().openAddToView([p.id]),
    onRemoveFromView:
      view && canRemove ? (p) => setMembership({ id: view.id, pipelineIds: [p.id], member: false }) : undefined,
    removeLabel: view && canRemove ? `Remove from ${view.name}` : undefined,
    onTrigger,
    onEnable: (p) => enableWorkflow(p),
    onViewMetrics,
    onViewHistory,
    onOpenView: openView,
    onRefreshProviders: (ids) => void refreshProviders(ids),
    isRefreshing: (ids) => ids.some((id) => refreshing.has(id)),
    pinItem,
    cancelItem,
  }

  const changeDisplay = (next: ViewDisplay) => {
    if (view) {
      updateView({ id: view.id, patch: { display: next } })
    }
  }

  return { actions, changeDisplay, cancelDialog }
}
