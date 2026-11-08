import { Alert, Stack, Text } from '@mantine/core'
import { IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { BuildWatchControls } from '../../../components/common/BuildWatchControls'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { RunDuration } from '../../../components/common/RunDuration'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineRun } from '../../../types/pipelines'
import type { PluginPresentation } from '../../../types/plugin_presentation'
import { isLiveRun } from '../../../utils/runTiming'
import { RunDetailFields, useDetailRows } from './RunDetailFields'

interface RunDetailsContentProps {
  pipelineId: string
  runNumber: number
  providerId: number | undefined
  run: PipelineRun | undefined
  error: Error | null
  loading: boolean
  fetching: boolean
  presentation: PluginPresentation | undefined
  isMobile: boolean
  mobileShell: boolean
  cancellationRequested: boolean
  onRetry: () => void
  children?: ReactNode
}

export function RunDetailsContent({
  runNumber,
  error,
  loading,
  fetching,
  run,
  onRetry,
  ...rest
}: RunDetailsContentProps): ReactNode {
  if (error) {
    return <ErrorState title={`Couldn't load run #${runNumber}`} error={error} onRetry={onRetry} retrying={fetching} />
  }

  if (loading || !run) {
    return <LoadingState message="Loading run details..." />
  }

  return <LoadedRunDetails run={run} runNumber={runNumber} {...rest} />
}

type LoadedProps = Omit<RunDetailsContentProps, 'error' | 'loading' | 'fetching' | 'run' | 'onRetry'> & {
  run: PipelineRun
}

function LoadedRunDetails({
  pipelineId,
  runNumber,
  providerId,
  run,
  presentation,
  isMobile,
  mobileShell,
  cancellationRequested,
  children,
}: LoadedProps): ReactNode {
  const rows = useDetailRows(providerId, run)
  const isRunning = run.status === 'running' || run.status === 'pending'
  const progress = isLiveRun(run) ? (
    <>
      Running for <RunDuration run={run} />. Details update live.
    </>
  ) : (
    'Details update live.'
  )

  const status = cancellationRequested
    ? 'Cancellation requested. Waiting for the provider to confirm the final status.'
    : progress

  const notices = (
    <>
      <BuildWatchControls pipelineId={pipelineId} runNumber={runNumber} status={run.status} />
      {run.metadata?.archived === true && presentation?.archived_notice && (
        <Alert color="gray" title={presentation.archived_notice.title}>
          {presentation.archived_notice.message}
        </Alert>
      )}
      {isRunning &&
        (mobileShell ? (
          <Alert color="blue" icon={<IconRefresh size={16} aria-hidden />} title={presentation?.progress_title}>
            <Text size={TYPOGRAPHY.HELPER_TEXT.size}>{progress}</Text>
          </Alert>
        ) : (
          <Alert color="gray" title={<StatusBadge status={run.status} />}>
            <Text size={TYPOGRAPHY.HELPER_TEXT.size} role="status">
              {status}
            </Text>
          </Alert>
        ))}
    </>
  )

  const compactGap = isMobile ? 'xs' : 'md'

  return (
    <Stack gap={mobileShell ? 'lg' : compactGap}>
      {notices}
      <RunDetailFields rows={rows} run={run} mobileShell={mobileShell} isMobile={isMobile} />
      {children}
    </Stack>
  )
}
