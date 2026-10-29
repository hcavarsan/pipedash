import type { ReactNode } from 'react'
import { PageContainer } from '../../../components/common/PageContainer'
import type { Pipeline, ProviderSummary } from '../../../types/domain'
import { usePipelineActions } from '../hooks/usePipelineActions'
import { usePipelineViewModel } from '../hooks/usePipelineViewModel'
import { PipelinePinnedArea } from './PipelinePinnedArea'
import { PipelineViewFilters } from './PipelineViewFilters'
import { PipelineViewResults } from './PipelineViewResults'
import { PipelineViewSelection } from './PipelineViewSelection'
import { PipelineBuildWatch, PipelineViewEmptyState, PipelineViewStatus } from './PipelineViewStatus'
import { PipelineViewNav, PipelineViewToolbar } from './PipelineViewToolbar'

interface UnifiedPipelinesViewProps {
  providers: ProviderSummary[]
  loading?: boolean
  error?: unknown
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
  onRetry,
  onViewHistory,
  onViewRun,
  onTrigger,
  onViewMetrics,
}: UnifiedPipelinesViewProps): ReactNode {
  const model = usePipelineViewModel({ providers, loading, error })
  const { actions, changeDisplay, cancelDialog } = usePipelineActions({
    view: model.view,
    onTrigger,
    onViewHistory,
    onViewMetrics,
  })

  return (
    <PipelineViewStatus model={model} onRetry={onRetry} onViewRun={onViewRun} onDisplayChange={changeDisplay}>
      <PageContainer>
        <PipelineViewToolbar model={model} onDisplayChange={changeDisplay} />
        <PipelineViewNav model={model} />
        <PipelineViewFilters model={model} />
        <PipelineBuildWatch model={model} onViewRun={onViewRun} />
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
