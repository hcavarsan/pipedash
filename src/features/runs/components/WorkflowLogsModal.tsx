import type { ReactNode } from 'react'
import { useState } from 'react'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { usePluginPresentation, usePlugins } from '../../../queries/usePluginsQueries'
import { useRunLogTree } from '../../../queries/useRunLogQueries'
import type { Pipeline } from '../../../types/pipelines'
import { getPluginDisplayName } from '../../../utils/pluginHelpers'
import { useRunDetailActions } from '../hooks/useRunDetailActions'
import { resolveLogGroups } from '../lib/logTree'
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
  pipeline: Pipeline | undefined
  canViewLogs: boolean
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
  const providerType = pipeline?.provider_type
  const presentation = usePluginPresentation(providerType)
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
    forceConfirmation: presentation?.force_cancel_confirmation,
    onClose,
    onRerunSuccess,
    onCancelSuccess,
    onCancelRequested: () => setCancellationRequested(true),
  })

  const requestRerun = () => {
    if (pipeline?.actions.rerun_strategy === 'in_place') {
      setConfirmingRerun(true)
    } else {
      actions.rerun('all').catch(() => undefined)
    }
  }

  const runActionProps = run && {
    run,
    providerName,
    isMobile,
    rerunLabel: presentation?.rerun.label ?? '',
    rerunCompactLabel: presentation?.rerun.compact_label ?? '',
    triggerNote: presentation?.trigger.note,
    canViewLogs,
    stopDisabled: !mobileShell && cancellationRequested,
    rerunPending: actions.rerunPending,
    cancelPending: actions.cancelPending,
    forceCancelPending: actions.forceCancelPending,
    onRerun: requestRerun,
    onCancel: () => {
      actions.cancel().catch(() => undefined)
    },
    onForceCancel: () => {
      actions.forceCancel().catch(() => undefined)
    },
  }
  const nodes = logTree?.nodes ?? []
  const groups = logTree ? resolveLogGroups(logTree, true) : []

  const details = (
    <ErrorBoundary scope="feature" name="Run details" resetKeys={[pipelineId, runNumber]}>
      <RunDetailsContent
        pipelineId={pipelineId}
        runNumber={runNumber}
        providerId={pipeline?.provider_id}
        run={run}
        error={error}
        loading={isLoading}
        fetching={isFetching}
        presentation={presentation}
        isMobile={isMobile}
        mobileShell={mobileShell}
        cancellationRequested={cancellationRequested}
        onRetry={() => {
          refetch().catch(() => undefined)
        }}
      >
        {mobileShell && runActionProps && (
          <>
            {canViewLogs && <MobileRunJobs groups={groups} onSelect={setLogNodeId} />}
            <MobileRunActions {...runActionProps} />
          </>
        )}
      </RunDetailsContent>
    </ErrorBoundary>
  )

  const title = presentation?.run_details_title
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
        onClose={() => setConfirmingRerun(false)}
        onConfirm={actions.rerun}
      />
    </>
  )
}
