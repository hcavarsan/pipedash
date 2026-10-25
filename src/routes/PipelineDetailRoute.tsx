import { useCallback, useEffect } from 'react'
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'

import { ErrorBoundary } from '../components/ErrorBoundary'
import { type DetailTab, RunHistoryPage } from '../components/table/RunHistoryPage'
import { usePlugins } from '../contexts/PluginContext'
import type { Pipeline, PipelineComponentProps } from '../types'
import { getReconcileBlockReason } from '../utils/pipelinePresentation'

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
  onRerunFailedJobs,
}: PipelineDetailRouteProps) {
  const { pipelineId } = useParams()
  const [searchParams, setSearchParams] = useSearchParams()
  const { state } = useLocation()
  const navigate = useNavigate()
  const { getPluginByType } = usePlugins()

  const rawTab = searchParams.get('tab')
  const tab: DetailTab = rawTab === 'metrics' ? 'metrics' : 'history'

  const setTab = useCallback(
    (next: DetailTab) => {
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev)

          if (next === 'metrics') {
            params.set('tab', 'metrics')
          } else {
            params.delete('tab')
          }

          return params
        },
        { replace: true, state }
      )
    },
    [setSearchParams, state]
  )

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

  useEffect(() => {
    if (rawTab !== null && rawTab !== 'metrics') {
      setTab('history')
    }
  }, [rawTab, setTab])

  return (
    <ErrorBoundary name="Pipeline history">
      <RunHistoryPage
        pipeline={pipeline}
        tab={tab}
        onTabChange={setTab}
        onBack={handleBack}
        onViewRun={onViewRun || (() => undefined)}
        onRerun={actualPipeline && getReconcileBlockReason(actualPipeline) ? undefined : onRerun}
        onCancel={
          actualPipeline && getPluginByType(actualPipeline.provider_type)?.capabilities.cancel ? onCancel : undefined
        }
        onReview={onReview}
        onRerunFailedJobs={onRerunFailedJobs}
        isLoadingPipeline={loading && !actualPipeline}
      />
    </ErrorBoundary>
  )
}
