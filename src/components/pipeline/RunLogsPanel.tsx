import { Box, Select } from '@mantine/core'
import { type Ref, useMemo, useState } from 'react'

import { useRunLogTree } from '../../queries/useRunLogQueries'
import type { RunLogNode } from '../../types/logs'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { LogViewer, type LogViewerHandle } from '../logs/LogViewer'

import { LogJobList } from './LogJobList'
import { groupLogNodes } from './logTree'

interface RunLogsPanelProps {
  pipelineId: string
  pipelineName: string
  runNumber: number
  providerName: string
  narrow: boolean
  viewerRef: Ref<LogViewerHandle>
}

export const RunLogsPanel = ({
  pipelineId,
  pipelineName,
  runNumber,
  providerName,
  narrow,
  viewerRef,
}: RunLogsPanelProps) => {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const { data: tree, isLoading, error, refetch, isFetching } = useRunLogTree(pipelineId, runNumber, true)

  const selectData = useMemo(() => {
    const groups = groupLogNodes(tree?.nodes ?? [])
    const items = (nodes: RunLogNode[]) => nodes.map((n) => ({ value: n.id, label: n.name }))

    return groups.length === 1 && groups[0].stage === null
      ? items(groups[0].nodes)
      : groups.map((group) => ({ group: group.stage ?? 'Jobs', items: items(group.nodes) }))
  }, [tree])

  if (isLoading) {
    return <LoadingState message="Loading jobs…" />
  }

  if (!tree) {
    return <ErrorState title="Couldn't load jobs" error={error} onRetry={() => refetch()} retrying={isFetching} />
  }

  const node =
    tree.nodes.find((n) => n.id === selectedId && n.log !== null) ??
    tree.nodes.find((n) => n.id === tree.default_node_id && n.log !== null)

  if (!node) {
    return <EmptyState title="This run has no jobs yet" description="Jobs appear here once the provider starts them." />
  }

  const viewer = (
    <LogViewer
      ref={viewerRef}
      pipelineId={pipelineId}
      pipelineName={pipelineName}
      runNumber={runNumber}
      node={node}
      steps={tree.nodes.filter((n) => n.parent_id === node.id && n.kind === 'step')}
      variant={narrow ? 'narrow' : 'desktop'}
      providerName={providerName}
    />
  )

  if (narrow) {
    return (
      <Box display="flex" flex={1} mih={0} style={{ flexDirection: 'column' }}>
        <Box p="sm" flex="none">
          <Select
            aria-label="Job"
            data={selectData}
            value={node.id}
            allowDeselect={false}
            onChange={(value) => setSelectedId(value)}
          />
        </Box>
        <Box flex={1} mih={0} display="flex" style={{ flexDirection: 'column' }}>
          {viewer}
        </Box>
      </Box>
    )
  }

  return (
    <Box display="flex" flex={1} mih={0}>
      <LogJobList nodes={tree.nodes} selectedId={node.id} onSelect={setSelectedId} />
      <Box flex={1} miw={0} mih={0} display="flex" style={{ flexDirection: 'column' }}>
        {viewer}
      </Box>
    </Box>
  )
}
