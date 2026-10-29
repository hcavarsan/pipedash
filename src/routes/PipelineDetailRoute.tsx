import { Button } from '@mantine/core'
import { type ReactNode, useCallback, useEffect } from 'react'
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'

import { EmptyState } from '../components/common/EmptyState'
import { PageContainer } from '../components/common/PageContainer'
import { PageHeader } from '../components/common/PageHeader'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { type DetailTab, RunHistoryPage } from '../components/table/RunHistoryPage'
import { usePlugins } from '../contexts/PluginContext'
import { useMobileShell } from '../hooks/useIsMobile'
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
}: PipelineDetailRouteProps): ReactNode {
  const { pipelineId } = useParams()
  const [searchParams, setSearchParams] = useSearchParams()
  const { state } = useLocation()
  const navigate = useNavigate()
  const { getPluginByType } = usePlugins()
  const mobileShell = useMobileShell()

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
      void navigate('/pipelines')
    }
  }

  useEffect(() => {
    if (mobileShell && !loading && !actualPipeline) {
      void navigate('/pipelines', { replace: true })
    }
  }, [mobileShell, loading, actualPipeline, navigate])

  useEffect(() => {
    if (rawTab !== null && rawTab !== 'metrics') {
      setTab('history')
    }
  }, [rawTab, setTab])

  if (!mobileShell && !loading && !actualPipeline) {
    return (
      <PageContainer>
        <PageHeader title="Pipeline unavailable" />
        <EmptyState
          title="Couldn't find this pipeline"
          description="It may have been removed or excluded from its provider. Return to Pipelines to choose another."
          action={
            <Button variant="default" onClick={handleBack}>
              Back to Pipelines
            </Button>
          }
        />
      </PageContainer>
    )
  }

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
