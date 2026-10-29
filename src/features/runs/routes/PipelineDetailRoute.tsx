import { Button } from '@mantine/core'
import type { ReactNode } from 'react'
import { Navigate, useLocation, useParams, useSearchParams } from 'react-router-dom'

import { EmptyState } from '../../../components/common/EmptyState'
import { PageContainer } from '../../../components/common/PageContainer'
import { PageHeader } from '../../../components/common/PageHeader'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { Pipeline } from '../../../types/domain'
import { RunActionsProvider } from '../components/RunActionsContext'
import { type DetailTab, RunHistoryPage } from '../components/RunHistoryPage'

interface PipelineDetailRouteProps {
  pipelines: Pipeline[]
  loading: boolean
  onBack: () => void
}

export function PipelineDetailRoute({ pipelines, loading, onBack }: PipelineDetailRouteProps): ReactNode {
  const { pipelineId } = useParams()
  const [searchParams, setSearchParams] = useSearchParams()
  const { state, pathname } = useLocation()
  const mobileShell = useMobileShell()

  const rawTab = searchParams.get('tab')
  const tab: DetailTab = rawTab === 'metrics' ? 'metrics' : 'history'

  const setTab = (next: DetailTab): void => {
    setSearchParams(
      (previous) => {
        const params = new URLSearchParams(previous)

        if (next === 'metrics') {
          params.set('tab', 'metrics')
        } else {
          params.delete('tab')
        }

        return params
      },
      { replace: true, state }
    )
  }

  const actualPipeline = pipelines.find((pipeline) => pipeline.id === pipelineId) ?? null

  const pipeline: Pipeline | null =
    actualPipeline ??
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

  if (rawTab !== null && rawTab !== 'metrics') {
    const params = new URLSearchParams(searchParams)
    params.delete('tab')

    return <Navigate replace state={state} to={{ pathname, search: params.toString() }} />
  }

  if (!loading && !actualPipeline) {
    if (mobileShell) {
      return <Navigate to="/pipelines" replace />
    }

    return (
      <PageContainer>
        <PageHeader title="Pipeline unavailable" />
        <EmptyState
          title="Couldn't find this pipeline"
          description="It may have been removed or excluded from its provider. Return to Pipelines to choose another."
          action={
            <Button variant="default" onClick={onBack}>
              Back to Pipelines
            </Button>
          }
        />
      </PageContainer>
    )
  }

  return (
    <ErrorBoundary scope="feature" name="Pipeline history" resetKeys={[pipelineId]}>
      <RunActionsProvider key={pipelineId} pipeline={actualPipeline}>
        <RunHistoryPage
          pipeline={pipeline}
          tab={tab}
          onTabChange={setTab}
          onBack={onBack}
          isLoadingPipeline={loading && !actualPipeline}
        />
      </RunActionsProvider>
    </ErrorBoundary>
  )
}
