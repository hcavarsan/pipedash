import { Alert, Stack, Text } from '@mantine/core'
import { IconRefresh } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { BuildWatchControls } from '../../../components/common/BuildWatchControls'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { RunDuration } from '../../../components/common/RunDuration'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineRun } from '../../../types/domain'
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
  isFlux: boolean
  isMobile: boolean
  mobileShell: boolean
  cancellationRequested: boolean
  typicalSeconds?: number | undefined
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
  isFlux,
  isMobile,
  mobileShell,
  cancellationRequested,
  typicalSeconds,
  children,
}: LoadedProps): ReactNode {
  const rows = useDetailRows(providerId, run)
  const isRunning = run.status === 'running' || run.status === 'pending'
  const progress = isLiveRun(run) ? (
    <>
      Running for <RunDuration run={run} typicalSeconds={typicalSeconds} />. Details update live.
    </>
  ) : (
    'Details update live.'
  )

  const notices = (
    <>
      <BuildWatchControls pipelineId={pipelineId} runNumber={runNumber} status={run.status} />
      {run.metadata?.archived === true && (
        <Alert color="gray" title="Cached reconciliation">
          Flux no longer retains this entry. These details come from Pipedash's cache and will disappear if the cache is
          cleared.
        </Alert>
      )}
      {isRunning &&
        (mobileShell ? (
          <Alert
            color="blue"
            icon={<IconRefresh size={16} aria-hidden />}
            title={isFlux ? 'Reconciliation in progress' : 'Run in progress'}
          >
            <Text size={TYPOGRAPHY.HELPER_TEXT.size}>{progress}</Text>
          </Alert>
        ) : (
          <Alert color="gray" title={<StatusBadge status={run.status} />}>
            <Text size={TYPOGRAPHY.HELPER_TEXT.size} role="status">
              {cancellationRequested
                ? 'Cancellation requested. Waiting for the provider to confirm the final status.'
                : progress}
            </Text>
          </Alert>
        ))}
    </>
  )

  return (
    <Stack gap={mobileShell ? 'lg' : isMobile ? 'xs' : 'md'}>
      {notices}
      <RunDetailFields rows={rows} run={run} mobileShell={mobileShell} isMobile={isMobile} />
      {children}
    </Stack>
  )
}
