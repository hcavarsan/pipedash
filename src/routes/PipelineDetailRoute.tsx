import { useEffect } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router-dom'

import { ErrorBoundary } from '../components/ErrorBoundary'
import { RunHistoryPage } from '../components/table/RunHistoryPage'
import type { Pipeline, PipelineComponentProps } from '../types'

interface PipelineDetailRouteProps extends Omit<PipelineComponentProps, 'pipeline'> {
  pipelines: Pipeline[]
}

export function PipelineDetailRoute({
  pipelines,
  loading,
  onBack,
  onViewRun,
  onRerun,
  onCancel,
  onReview,
}: PipelineDetailRouteProps) {
  const { pipelineId } = useParams()
  const [searchParams] = useSearchParams()
  const navigate = useNavigate()

  const tab = (searchParams.get('tab') || 'history') as 'history' | 'metrics'

  const actualPipeline = pipelines.find((p) => p.id === pipelineId) || null

  const pipeline: Pipeline | null =
    actualPipeline ||
    (loading && pipelineId
      ? {
          id: pipelineId,
          name: pipelineId,
          provider_id: 0,
          repository: '',
          branch: null,
          status: 'pending' as const,
          last_run: null,
          last_updated: new Date().toISOString(),
          workflow_file: null,
          provider_type: 'github',
        }
      : null)

  const handleBack = () => {
    if (onBack) {
      onBack()
    } else {
      navigate('/pipelines')
    }
  }

  useEffect(() => {
    if (!loading && !actualPipeline) {
      navigate('/pipelines', { replace: true })
    }
  }, [loading, actualPipeline, navigate])

  return (
    <ErrorBoundary name="Pipeline history">
      <RunHistoryPage
        pipeline={pipeline}
        initialTab={tab}
        onBack={handleBack}
        onViewRun={onViewRun || (() => undefined)}
        onRerun={onRerun}
        onCancel={onCancel}
        onReview={onReview}
        isLoadingPipeline={loading && !actualPipeline}
      />
    </ErrorBoundary>
  )
}
