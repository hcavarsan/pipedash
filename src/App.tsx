import { useState } from 'react'

import { Container } from '@mantine/core'
import { notifications } from '@mantine/notifications'

import { AppLayout } from './components/layout/AppLayout'
import { TriggerWorkflowModal } from './components/pipeline/TriggerWorkflowModal'
import { WorkflowLogsModal } from './components/pipeline/WorkflowLogsModal'
import { RunHistoryPage } from './components/table/RunHistoryPage'
import { UnifiedPipelinesView } from './components/table/UnifiedPipelinesView'
import { usePipelines } from './hooks/usePipelines'
import { useProviders } from './hooks/useProviders'
import { tauriService } from './services/tauri'
import type { Pipeline, PipelineRun } from './types'

type View = 'pipelines' | 'run-history';

function App() {
  const [selectedProviderId, setSelectedProviderId] = useState<number | undefined>(undefined)
  const [currentView, setCurrentView] = useState<View>('pipelines')
  const [historyPipeline, setHistoryPipeline] = useState<Pipeline | null>(null)
  const [initialHistoryTab, setInitialHistoryTab] = useState<'history' | 'metrics'>('history')
  const [triggerPipeline, setTriggerPipeline] = useState<Pipeline | null>(null)
  const [triggerInputs, setTriggerInputs] = useState<Record<string, any> | undefined>(undefined)
  const [logsModal, setLogsModal] = useState<{ opened: boolean; pipelineId: string; runNumber: number }>({
    opened: false,
    pipelineId: '',
    runNumber: 0,
  })
  const [refreshTrigger, setRefreshTrigger] = useState(0)
  const [runHistoryLoading, setRunHistoryLoading] = useState(false)
  const [isRefreshing, setIsRefreshing] = useState(false)

  const { pipelines, loading: pipelinesLoading, refresh: refreshPipelines } = usePipelines(selectedProviderId)
  const { providers, loading: providersLoading, addProvider, updateProvider, removeProvider, refresh: refreshProviders } = useProviders()

  const handleRefreshAll = async () => {
    setIsRefreshing(true)
    try {
      await Promise.all([
        refreshProviders(),
        refreshPipelines()
      ])
      setRefreshTrigger(prev => prev + 1)
    } finally {
      setTimeout(() => setIsRefreshing(false), 300)
    }
  }

  const filteredPipelines = selectedProviderId
    ? pipelines.filter((p) => p.provider_id === selectedProviderId)
    : pipelines

  const handleViewHistory = (pipeline: Pipeline) => {
    setHistoryPipeline(pipeline)
    setInitialHistoryTab('history')
    setCurrentView('run-history')
  }

  const handleViewMetrics = (pipeline: Pipeline) => {
    setHistoryPipeline(pipeline)
    setInitialHistoryTab('metrics')
    setCurrentView('run-history')
  }

  const handleBackFromHistory = () => {
    setHistoryPipeline(null)
    setInitialHistoryTab('history')
    setCurrentView('pipelines')
  }

  const handleProviderSelect = (id: number | undefined) => {
    setSelectedProviderId(id)
    setCurrentView('pipelines')
    setHistoryPipeline(null)
  }

  const handleRerun = async (pipeline: Pipeline, run: PipelineRun) => {
    setTriggerPipeline(pipeline)
    setTriggerInputs(undefined)

    try {
      const runDetails = await tauriService.getWorkflowRunDetails(
        run.pipeline_id,
        run.run_number
      )

      const inputs = runDetails.inputs || {}

      if (run.branch && !inputs.branch && !inputs.ref) {
        inputs.ref = run.branch
      }

      setTriggerInputs(inputs)
    } catch (error: any) {
      console.error('[Rerun] Failed to fetch run details:', error)
      const errorMsg = error?.error || error?.message || 'Failed to load run details'

      setTriggerPipeline(null)
      setTriggerInputs(undefined)

      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    }
  }

  const handleCloseTriggerModal = () => {
    setTriggerPipeline(null)
    setTriggerInputs(undefined)
  }

  const handleCancel = async (_pipeline: Pipeline, run: PipelineRun) => {
    try {
      console.log('[Cancel] Cancelling run #', run.run_number)

      await tauriService.cancelPipelineRun(run.pipeline_id, run.run_number)

      console.log('[Cancel] Waiting for backend to update status...')
      let attempts = 0
      const maxAttempts = 10
      let statusUpdated = false

      while (attempts < maxAttempts && !statusUpdated) {
        await new Promise(resolve => setTimeout(resolve, 1000))

        try {
          const runDetails = await tauriService.getWorkflowRunDetails(run.pipeline_id, run.run_number)


          if (runDetails && (runDetails.status === 'cancelled' as any)) {
            statusUpdated = true
            console.log(`[Cancel] Status updated after ${attempts + 1} seconds`)
          }
        } catch (_error) {
          console.warn(`[Cancel] Polling attempt ${attempts + 1} failed, continuing...`)
        }

        attempts++
      }

      if (!statusUpdated) {
        console.warn('[Cancel] Status not confirmed after 10s, refreshing anyway')
      }

      await handleRefreshAll()
    } catch (error: any) {
      console.error('[Cancel] Failed to cancel run:', error)
      const errorMsg = error?.error || error?.message || 'Failed to cancel run'

      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    }
  }

  return (
    <AppLayout
      selectedProviderId={selectedProviderId}
      onProviderSelect={handleProviderSelect}
      providers={providers}
      onAddProvider={addProvider}
      onUpdateProvider={updateProvider}
      onRemoveProvider={removeProvider}
      onRefreshAll={handleRefreshAll}
      onRefreshProviders={refreshProviders}
      refreshing={isRefreshing || pipelinesLoading || providersLoading || runHistoryLoading}
    >
      {currentView === 'run-history' ? (
        <RunHistoryPage
          pipeline={historyPipeline}
          onBack={handleBackFromHistory}
          onViewRun={(pipelineId, runNumber) => {
            setLogsModal({ opened: true, pipelineId, runNumber })
          }}
          onRerun={handleRerun}
          onCancel={handleCancel}
          refreshTrigger={refreshTrigger}
          onLoadingChange={setRunHistoryLoading}
          initialTab={initialHistoryTab}
        />
      ) : (
        <Container size="100%" pt={{ base: 'xs', sm: 'md' }} pb={{ base: 'xs', sm: '2xl' }} px={{ base: 'xs', sm: 'xl' }}>
          <UnifiedPipelinesView
            pipelines={filteredPipelines}
            providers={providers}
            loading={providersLoading}
            onViewHistory={handleViewHistory}
            onViewMetrics={handleViewMetrics}
            onTrigger={setTriggerPipeline}
          />
        </Container>
      )}

      {triggerPipeline && (
        <TriggerWorkflowModal
          opened={!!triggerPipeline}
          onClose={handleCloseTriggerModal}
          pipeline={triggerPipeline}
          initialInputs={triggerInputs}
          onSuccess={async (pipelineId, runNumber) => {
            await handleRefreshAll()
            if (triggerPipeline) {
              handleViewHistory(triggerPipeline)
            }
            setLogsModal({ opened: true, pipelineId, runNumber })
          }}
        />
      )}

      <WorkflowLogsModal
        opened={logsModal.opened}
        onClose={() => setLogsModal({ opened: false, pipelineId: '', runNumber: 0 })}
        pipelineId={logsModal.pipelineId}
        runNumber={logsModal.runNumber}
        onRerunSuccess={async (pipelineId, newRunNumber) => {
          await handleRefreshAll()
          setLogsModal({ opened: true, pipelineId, runNumber: newRunNumber })
        }}
        onCancelSuccess={async () => {
          await handleRefreshAll()
        }}
      />
    </AppLayout>
  )
}

export default App
