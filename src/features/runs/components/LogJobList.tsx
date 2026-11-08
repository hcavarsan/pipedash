import { Box } from '@mantine/core'
import type { ReactNode } from 'react'
import { FillScrollArea } from '../../../components/common/FillScrollArea'
import { VirtualList } from '../../../components/common/VirtualList'
import type { RunLogNode } from '../../../types/logs'
import { LogJobItem } from './LogJobSummary'

interface LogJobListProps {
  nodes: readonly RunLogNode[]
  selectedId: string | null
  onSelect: (id: string) => void
}

export const LogJobList = ({ nodes, selectedId, onSelect }: LogJobListProps): ReactNode => (
  <FillScrollArea w={260} style={{ flex: 'none', borderRight: '1px solid var(--mantine-color-default-border)' }}>
    <Box component="nav" aria-label="Jobs" p="xs">
      <VirtualList
        items={nodes.filter((node) => node.navigable)}
        getKey={(node) => node.id}
        estimateSize={36}
        renderItem={(node) => <LogJobItem node={node} selectedId={selectedId} onSelect={onSelect} />}
      />
    </Box>
  </FillScrollArea>
)
