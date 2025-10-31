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
  const [triggerPipeline, setTriggerPipeline] = useState<Pipeline | null>(null)
  const [triggerInputs, setTriggerInputs] = useState<Record<string, any> | undefined>(undefined)
  const [logsModal, setLogsModal] = useState<{ opened: boolean; pipelineId: string; runNumber: number }>({
    opened: false,
    pipelineId: '',
    runNumber: 0,
  })
  const [refreshTrigger, setRefreshTrigger] = useState(0)

  const { pipelines, refresh: refreshPipelines } = usePipelines(selectedProviderId)
  const { providers, addProvider, updateProvider, removeProvider, refresh: refreshProviders } = useProviders()

  const handleRefreshCurrent = async () => {
    if (currentView === 'run-history') {
      setRefreshTrigger(prev => prev + 1)
    } else {
      await refreshPipelines()
    }
  }

  const handleRefreshAll = async () => {
    await refreshPipelines()
    setRefreshTrigger(prev => prev + 1)
  }

  const filteredPipelines = selectedProviderId
    ? pipelines.filter((p) => p.provider_id === selectedProviderId)
    : pipelines

  const handleViewHistory = (pipeline: Pipeline) => {
    setHistoryPipeline(pipeline)
    setCurrentView('run-history')
  }

  const handleBackFromHistory = () => {
    setHistoryPipeline(null)
    setCurrentView('pipelines')
  }

  const handleProviderSelect = (id: number | undefined) => {
    setSelectedProviderId(id)
    setCurrentView('pipelines')
    setHistoryPipeline(null)
  }

  const handleRerun = async (pipeline: Pipeline, run: PipelineRun) => {
    try {
      console.log('[Rerun] Fetching details for run #', run.run_number)

      setTriggerPipeline(pipeline)

      const runDetails = await tauriService.getWorkflowRunDetails(
        run.pipeline_id,
        run.run_number
      )

      console.log('[Rerun] Fetched run details:', runDetails)
      console.log('[Rerun] Inputs:', runDetails.inputs)

      setTriggerInputs(runDetails.inputs)
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
      onRefreshCurrent={handleRefreshCurrent}
      onRefreshAll={handleRefreshAll}
      onRefreshProviders={refreshProviders}
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
        />
      ) : (
        <Container size="100%" py={{ base: 'xs', sm: 'md' }} px={{ base: 'xs', sm: 'xl' }}>
          <UnifiedPipelinesView
            pipelines={filteredPipelines}
            providers={providers}
            onViewHistory={handleViewHistory}
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
          onSuccess={(pipelineId, runNumber) => {
            // Navigate to run history page
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
        onRerunSuccess={(pipelineId, newRunNumber) => {
          setLogsModal({ opened: true, pipelineId, runNumber: newRunNumber })
        }}
        onCancelSuccess={() => {
          setRefreshTrigger(prev => prev + 1)
        }}
      />
    </AppLayout>
  )
}

export default App
