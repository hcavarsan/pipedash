import { Button, Stack } from '@mantine/core'
import { IconExternalLink } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useRetryRunLog, useRunLogDocument } from '../../../queries/useRunLogQueries'
import { formatBytes } from '../../../utils/formatBytes'
import { openLink } from '../../../utils/openExternal'
import type { LogViewerProps } from '../lib/logViewerTypes'
import { LiveMarker } from './LiveMarker'
import { PendingLogSteps } from './PendingLogSteps'
import { ReadyViewer } from './ReadyViewer'

export function LogViewer({ ref, ...props }: LogViewerProps): ReactNode {
  return (
    <ErrorBoundary key={`${props.pipelineId}:${props.runNumber}:${props.node.id}`} scope="feature" name="Job log">
      <LogSession {...props} ref={ref} />
    </ErrorBoundary>
  )
}

function OpenJobButton({ node, providerName }: Pick<LogViewerProps, 'node' | 'providerName'>): ReactNode {
  const url = node.web_url

  return url ? (
    <Button
      variant="default"
      size="xs"
      leftSection={<IconExternalLink size={14} aria-hidden />}
      onClick={() => openLink(url)}
    >
      Open job in {providerName}
    </Button>
  ) : null
}

function LogSession({ ref, ...props }: LogViewerProps): ReactNode {
  const { pipelineId, runNumber, node, steps, providerName } = props
  const docQuery = useRunLogDocument(pipelineId, runNumber, node.id, true)
  const retry = useRetryRunLog()
  const data = docQuery.data
  const open = <OpenJobButton node={node} providerName={providerName} />

  if (!data) {
    if (docQuery.error) {
      return (
        <ErrorState
          title="Couldn't load the job log"
          error={docQuery.error}
          actions={open}
          retrying={docQuery.isFetching}
          onRetry={() => void docQuery.refetch()}
        />
      )
    }

    return <LoadingState message="Preparing log…" />
  }

  if (data.state === 'loading') {
    return (
      <LoadingState
        message={data.received_bytes ? `Downloading log… ${formatBytes(data.received_bytes)}` : 'Preparing log…'}
      />
    )
  }

  if (data.state === 'pending') {
    return (
      <EmptyState
        title="Logs appear when this job finishes"
        description="This job is still running. Its log opens here by itself."
        action={
          <Stack gap="xs" align="stretch" w="100%" maw={420}>
            <PendingLogSteps steps={steps} />
            {open}
          </Stack>
        }
      />
    )
  }

  if (data.state === 'unavailable') {
    return (
      <ErrorState
        title="Couldn't load the job log"
        error={retry.error ?? data.message}
        actions={open}
        retrying={retry.isPending}
        onRetry={() => retry.mutate({ pipelineId, runNumber, nodeId: node.id })}
      />
    )
  }

  if (data.line_count === 0) {
    if (data.complete) {
      return <EmptyState title="This job didn't write any output" description="The provider returned an empty log." />
    }

    return (
      <Stack gap={0}>
        {data.refresh_error !== null && (
          <LiveMarker meta={data} target={{ pipelineId, runNumber, nodeId: node.id }} variant={props.variant} />
        )}
        <LoadingState message="Waiting for output…" />
      </Stack>
    )
  }

  return <ReadyViewer {...props} meta={data} ref={ref} />
}
