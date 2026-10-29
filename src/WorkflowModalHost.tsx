import { lazy, type ReactNode, Suspense } from 'react'

import { LoadingState } from './components/common/LoadingState'
import { ErrorBoundary } from './components/ErrorBoundary'
import { usePlugins } from './queries/usePluginsQueries'
import { useRefreshProvider } from './queries/useRefreshQueries'
import { useModalStore } from './stores/modalStore'
import type { Pipeline } from './types'
import { canRerunInPlace, getReconcileBlockReason } from './utils/pipelinePresentation'
import { getPluginByType } from './utils/pluginHelpers'

const TriggerWorkflowModal = lazy(() =>
  import('./components/pipeline/TriggerWorkflowModal').then((m) => ({
    default: m.TriggerWorkflowModal,
  }))
)

const WorkflowLogsModal = lazy(() =>
  import('./components/pipeline/WorkflowLogsModal').then((m) => ({
    default: m.WorkflowLogsModal,
  }))
)

interface WorkflowModalHostProps {
  pipelines: Pipeline[]
  invalidateAll: () => Promise<void>
}

export function WorkflowModalHost({ pipelines, invalidateAll }: WorkflowModalHostProps): ReactNode {
  const { data: plugins } = usePlugins()
  const { mutate: refreshProvider } = useRefreshProvider()
  const triggerModal = useModalStore((s) => s.triggerModal)
  const logsModal = useModalStore((s) => s.logsModal)
  const openLogsModal = useModalStore((s) => s.openLogsModal)
  const closeTriggerModal = useModalStore((s) => s.closeTriggerModal)
  const closeLogsModal = useModalStore((s) => s.closeLogsModal)

  const syncAfterAction = async (providerId?: number): Promise<void> => {
    if (providerId !== undefined) {
      refreshProvider(providerId)
    }
    await invalidateAll()
  }

  const triggerPipeline = triggerModal.open ? pipelines.find((p) => p.id === triggerModal.pipelineId) : undefined
  const logsPipeline = logsModal.open ? pipelines.find((p) => p.id === logsModal.pipelineId) : undefined

  return (
    <>
      {triggerModal.open && triggerPipeline && (
        <ErrorBoundary scope="feature" name="Trigger workflow">
          <Suspense fallback={<LoadingState message="Loading dialog..." />}>
            <TriggerWorkflowModal
              opened
              onClose={closeTriggerModal}
              pipeline={triggerPipeline}
              initialInputs={triggerModal.initialInputs}
              onSuccess={async (pipelineId, runNumber) => {
                await syncAfterAction(triggerPipeline.provider_id)
                openLogsModal(pipelineId, runNumber)
              }}
            />
          </Suspense>
        </ErrorBoundary>
      )}

      {logsModal.open && (
        <ErrorBoundary scope="feature" name="Run details">
          <Suspense fallback={<LoadingState message="Loading dialog..." />}>
            <WorkflowLogsModal
              opened
              onClose={closeLogsModal}
              pipelineId={logsModal.pipelineId}
              runNumber={logsModal.runNumber}
              providerId={logsPipeline?.provider_id}
              providerType={logsPipeline?.provider_type}
              pipeline={logsPipeline}
              canViewLogs={!!logsPipeline && !!getPluginByType(plugins, logsPipeline.provider_type)?.capabilities.logs}
              canTrigger={!!logsPipeline && !getReconcileBlockReason(logsPipeline)}
              triggerBlockedReason={logsPipeline ? getReconcileBlockReason(logsPipeline) : undefined}
              rerunInPlace={!!logsPipeline && canRerunInPlace(logsPipeline)}
              canCancel={!!logsPipeline && !!getPluginByType(plugins, logsPipeline.provider_type)?.capabilities.cancel}
              onRerunSuccess={async (pipelineId, newRunNumber) => {
                await syncAfterAction(logsPipeline?.provider_id)
                openLogsModal(pipelineId, newRunNumber)
              }}
              onCancelSuccess={() => syncAfterAction(logsPipeline?.provider_id)}
            />
          </Suspense>
        </ErrorBoundary>
      )}
    </>
  )
}
