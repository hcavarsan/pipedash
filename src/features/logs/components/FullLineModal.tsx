import { Alert, Button, Code, Group, Stack } from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { CopyButton } from '../../../components/atoms/CopyButton'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import { useLogExport } from '../hooks/useLogExport'
import type { DocRef } from '../lib/lineCacheTypes'
import { FULL_LINE_UNITS } from '../lib/logActions'

interface FullLineModalProps {
  doc: DocRef
  line: number
  pipelineName: string
  nodeName: string
  onClose: () => void
}

function FullLineContent({ doc, line, pipelineName, nodeName }: Omit<FullLineModalProps, 'onClose'>): ReactNode {
  const { pipelineId, runNumber, nodeId, revision } = doc
  const { exportLog, exporting } = useLogExport({ doc, pipelineName, nodeName, selection: null })
  const query = useQuery({
    queryKey: [...queryKeys.runLogs.all, 'line', pipelineId, runNumber, nodeId, revision, line],
    queryFn: () => service.readRunLogLines(pipelineId, runNumber, nodeId, revision, line, 1, FULL_LINE_UNITS),
    gcTime: 0,
    staleTime: Infinity,
    retry: false,
  })
  const text = query.data?.lines[0]?.text ?? ''

  if (query.isPending) {
    return <LoadingState message="Loading line…" />
  }

  if (query.error) {
    return <ErrorState title="Couldn't load the line" error={query.error} onRetry={() => void query.refetch()} />
  }

  const omitted = query.data?.lines[0]?.omitted ?? 0

  return (
    <Stack gap="xs">
      {omitted > 0 && (
        <Alert color="orange" variant="light">
          <Stack gap="xs" align="flex-start">
            This line is longer than Pipedash can show. Export the log to get all of it.
            <Button type="button" size="xs" variant="default" loading={exporting} onClick={() => void exportLog()}>
              {exporting ? 'Exporting log…' : 'Export log'}
            </Button>
          </Stack>
        </Alert>
      )}
      <Group justify="flex-end">
        <CopyButton value={text} label={omitted > 0 ? 'Copy shown text' : 'Copy line'} />
      </Group>
      <Code block mah={400} style={{ overflow: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
        {text}
      </Code>
    </Stack>
  )
}

export function FullLineModal({ doc, line, pipelineName, nodeName, onClose }: FullLineModalProps): ReactNode {
  return (
    <StandardModal opened onClose={onClose} title={`Line ${line + 1}`} size="lg">
      <ErrorBoundary scope="feature" name="Full log line">
        <FullLineContent doc={doc} line={line} pipelineName={pipelineName} nodeName={nodeName} />
      </ErrorBoundary>
    </StandardModal>
  )
}
