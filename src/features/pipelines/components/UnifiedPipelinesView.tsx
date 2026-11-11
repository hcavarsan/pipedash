import { type ReactNode, useState } from 'react'
import { PageContainer } from '../../../components/common/PageContainer'
import type { Pipeline } from '../../../types/pipelines'
import type { ProviderSummary } from '../../../types/providers'
import { usePipelineActions } from '../hooks/usePipelineActions'
import { usePipelineViewModel } from '../hooks/usePipelineViewModel'
import { PipelinePinnedArea } from './PipelinePinnedArea'
import { PipelineViewFilters } from './PipelineViewFilters'
import { PipelineViewResults } from './PipelineViewResults'
import { PipelineViewSelection } from './PipelineViewSelection'
import {
  PipelineBuildWatch,
  PipelineViewEmptyState,
  PipelineViewReloadError,
  PipelineViewStatus,
} from './PipelineViewStatus'
import { PipelineViewNav, PipelineViewToolbar } from './PipelineViewToolbar'

interface UnifiedPipelinesViewProps {
  providers: ProviderSummary[]
  loading?: boolean
  error?: unknown
  retrying?: boolean
  onRetry: () => void
  onViewHistory: (pipeline: Pipeline) => void
  onViewRun: (pipelineId: string, runNumber: number) => void
  onTrigger: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function UnifiedPipelinesView({
  providers,
  loading = false,
  error,
  retrying = false,
  onRetry,
  onViewHistory,
  onViewRun,
  onTrigger,
  onViewMetrics,
}: UnifiedPipelinesViewProps): ReactNode {
  const model = usePipelineViewModel({ providers, loading, error, retrying })
  const [filtersOpen, setFiltersOpen] = useState(false)
  const { actions, changeDisplay, cancelDialog, viewUpdating } = usePipelineActions({
    view: model.view,
    onTrigger,
    onViewHistory,
    onViewMetrics,
  })
  const nav = <PipelineViewNav model={model} />
  const filters = (
    <PipelineViewFilters model={model} sheet={{ opened: filtersOpen, onClose: () => setFiltersOpen(false) }} />
  )

  let header: ReactNode = null

  if (!model.pushed) {
    header = model.mobileShell ? (
      <>
        {filters}
        {nav}
      </>
    ) : (
      <>
        {nav}
        {filters}
      </>
    )
  }

  return (
    <PipelineViewStatus model={model} onRetry={onRetry} onViewRun={onViewRun} onDisplayChange={changeDisplay}>
      <PageContainer>
        <PipelineViewToolbar
          model={model}
          viewUpdating={viewUpdating}
          onDisplayChange={changeDisplay}
          onOpenFilters={() => setFiltersOpen(true)}
        />
        {header}
        <PipelineBuildWatch model={model} onViewRun={onViewRun} />
        <PipelineViewReloadError model={model} onRetry={onRetry} />
        <PipelineViewSelection model={model} actions={actions} />
        {cancelDialog}
        <PipelinePinnedArea model={model} actions={actions} />
        <PipelineViewResults
          model={model}
          actions={actions}
          emptyState={<PipelineViewEmptyState model={model} onRetry={onRetry} />}
          onDisplayChange={changeDisplay}
        />
      </PageContainer>
    </PipelineViewStatus>
  )
}
