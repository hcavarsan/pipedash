import type { ReactNode } from 'react'
import { useState } from 'react'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { usePlugins } from '../../../queries/usePluginsQueries'
import { useRunLogTree } from '../../../queries/useRunLogQueries'
import type { Pipeline } from '../../../types/domain'
import { getPipelineLabels } from '../../../utils/pipelinePresentation'
import { getPluginDisplayName } from '../../../utils/pluginHelpers'
import { useRunDetailActions } from '../hooks/useRunDetailActions'
import { useRunDetails } from '../queries/useRunDetailsQuery'
import { MobileRunJobs } from './MobileRunJobs'
import { MobileRunLogModal } from './MobileRunLogModal'
import { RerunConfirmModal } from './RerunConfirmModal'
import { MobileRunActions, RunActionsFooter } from './RunActionsFooter'
import { RunDetailsContent } from './RunDetailsContent'
import { WorkflowLogTabs } from './WorkflowLogTabs'

interface WorkflowLogsModalProps {
  opened: boolean
  onClose: () => void
  pipelineId: string
  runNumber: number
  providerId?: number | undefined
  providerType?: string | undefined
  pipeline: Pipeline | undefined
  canViewLogs: boolean
  canTrigger: boolean
  triggerBlockedReason?: string | undefined
  canCancel: boolean
  rerunInPlace?: boolean | undefined
  onRerunSuccess?: ((pipelineId: string, newRunNumber: number) => void) | undefined
  onCancelSuccess?: (() => void) | undefined
}

export const WorkflowLogsModal = (props: WorkflowLogsModalProps): ReactNode => (
  <WorkflowLogsSession key={`${props.pipelineId}:${props.runNumber}`} {...props} />
)

function WorkflowLogsSession({
  opened,
  onClose,
  pipelineId,
  runNumber,
  providerId,
  providerType,
  canTrigger,
  triggerBlockedReason,
  canCancel,
  rerunInPlace,
  onRerunSuccess,
  onCancelSuccess,
  pipeline,
  canViewLogs,
}: WorkflowLogsModalProps): ReactNode {
  const pipelineName = pipeline?.name ?? ''
  const [confirmingRerun, setConfirmingRerun] = useState(false)
  const [cancellationRequested, setCancellationRequested] = useState(false)
  const [logNodeId, setLogNodeId] = useState<string | null>(null)
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const labels = getPipelineLabels(providerType)
  const isFlux = providerType === 'fluxcd'
  const { data: plugins } = usePlugins()
  const providerName = providerType ? getPluginDisplayName(plugins, providerType) : 'provider'
  const { data: logTree } = useRunLogTree(pipelineId, runNumber, canViewLogs && mobileShell && opened)
  const {
    data: run,
    isLoading,
    isFetching,
    error,
    refetch,
  } = useRunDetails(pipelineId, runNumber, opened, cancellationRequested)
  const actions = useRunDetailActions({
    pipelineId,
    runNumber,
    pipeline,
    run,
    rerunInPlace: rerunInPlace === true,
    onClose,
    onRerunSuccess,
    onCancelSuccess,
    onCancelRequested: () => setCancellationRequested(true),
  })

  const requestRerun = () => {
    if (isFlux) {
      actions.rerun('all').catch(() => undefined)
    } else {
      setConfirmingRerun(true)
    }
  }

  const runActionProps = run && {
    run,
    providerName,
    isMobile,
    isFlux,
    triggerLabel: labels.trigger,
    canViewLogs,
    canTrigger,
    canCancel,
    triggerBlockedReason,
    stopDisabled: !mobileShell && cancellationRequested,
    rerunPending: actions.rerunPending,
    cancelPending: actions.cancelPending,
    onRerun: requestRerun,
    onCancel: () => {
      actions.cancel().catch(() => undefined)
    },
  }
  const nodes = logTree?.nodes ?? []

  const details = (
    <ErrorBoundary scope="feature" name="Run details" resetKeys={[pipelineId, runNumber]}>
      <RunDetailsContent
        pipelineId={pipelineId}
        runNumber={runNumber}
        providerId={providerId}
        run={run}
        error={error}
        loading={isLoading}
        fetching={isFetching}
        isFlux={isFlux}
        isMobile={isMobile}
        mobileShell={mobileShell}
        cancellationRequested={cancellationRequested}
        onRetry={() => {
          refetch().catch(() => undefined)
        }}
      >
        {mobileShell && runActionProps && (
          <>
            {canViewLogs && <MobileRunJobs nodes={nodes} onSelect={setLogNodeId} />}
            <MobileRunActions {...runActionProps} />
          </>
        )}
      </RunDetailsContent>
    </ErrorBoundary>
  )

  const title = isFlux ? 'Reconciliation details' : 'Run details'
  const footer = runActionProps ? <RunActionsFooter {...runActionProps} /> : null

  return (
    <>
      {canViewLogs && !mobileShell ? (
        <StandardModal
          opened={opened}
          onClose={onClose}
          size="full"
          title={title}
          footer={footer}
          disableScrollArea
          contentPadding={false}
        >
          <WorkflowLogTabs
            pipelineId={pipelineId}
            pipelineName={pipelineName}
            runNumber={runNumber}
            providerName={providerName}
            narrow={isMobile}
            details={details}
          />
        </StandardModal>
      ) : (
        <StandardModal
          opened={opened}
          onClose={onClose}
          size="lg"
          title={title}
          footer={mobileShell ? undefined : footer}
          fullHeight
        >
          {details}
        </StandardModal>
      )}
      {mobileShell && (
        <MobileRunLogModal
          pipelineId={pipelineId}
          pipelineName={pipelineName}
          runNumber={runNumber}
          providerName={providerName}
          nodes={nodes}
          selectedId={logNodeId}
          onClose={() => setLogNodeId(null)}
        />
      )}
      <RerunConfirmModal
        run={confirmingRerun && run ? run : null}
        pipeline={pipeline}
        providerName={providerName}
        onClose={() => setConfirmingRerun(false)}
        onConfirm={actions.rerun}
      />
    </>
  )
}
