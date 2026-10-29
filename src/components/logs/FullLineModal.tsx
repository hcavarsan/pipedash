import { Code, Group, Stack } from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { queryKeys } from '../../lib/queryKeys'
import { service } from '../../services'
import { CopyButton } from '../atoms/CopyButton'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'
import type { DocRef } from './lineCache'
import { FULL_LINE_UNITS } from './logActions'

interface FullLineModalProps {
  doc: DocRef
  line: number
  onClose: () => void
}

function FullLineContent({ doc, line }: Omit<FullLineModalProps, 'onClose'>): ReactNode {
  const { pipelineId, runNumber, nodeId, revision } = doc
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

  return (
    <Stack gap="xs">
      <Group justify="flex-end">
        <CopyButton value={text} label="Copy line" />
      </Group>
      <Code block mah={400} style={{ overflow: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
        {text}
      </Code>
    </Stack>
  )
}

export function FullLineModal({ doc, line, onClose }: FullLineModalProps): ReactNode {
  return (
    <StandardModal opened onClose={onClose} title={`Line ${line + 1}`} size="lg">
      <ErrorBoundary scope="feature" name="Full log line">
        <FullLineContent doc={doc} line={line} />
      </ErrorBoundary>
    </StandardModal>
  )
}
