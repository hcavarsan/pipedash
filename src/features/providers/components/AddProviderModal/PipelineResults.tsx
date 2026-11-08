import { Box, Button } from '@mantine/core'
import { IconSearch, IconStack2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../../components/common/EmptyState'
import { VirtualList } from '../../../../components/common/VirtualList'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import {
  getPipelineId,
  MobilePipelineCard,
  type PipelineListProps,
  PipelinesTable,
  ShellPipelineList,
} from './PipelineSelectionLists'

interface PipelineResultsProps extends PipelineListProps {
  searchActive: boolean
  isMobile: boolean
  onClearSearch: () => void
}

export function PipelineResults({ searchActive, isMobile, onClearSearch, ...list }: PipelineResultsProps): ReactNode {
  const mobileShell = useMobileShell()
  const { pipelines, selectedPipelines, dispatch, terms, onEndReached } = list

  if (pipelines.length === 0) {
    return searchActive ? (
      <EmptyState
        icon={<IconSearch size={24} aria-hidden />}
        title={`No ${terms.items} match your search`}
        description="Try a different name, type or source."
        action={
          <Button variant="default" size="sm" onClick={onClearSearch}>
            Clear search
          </Button>
        }
      />
    ) : (
      <EmptyState
        icon={<IconStack2 size={24} aria-hidden />}
        title={`No ${terms.items} found`}
        description={
          mobileShell
            ? `This ${terms.group} has no accessible ${terms.items}.`
            : `This ${terms.group} has no accessible ${terms.items}. Go back to Credentials to check scope and permissions.`
        }
      />
    )
  }

  if (mobileShell) {
    return <ShellPipelineList {...list} />
  }

  if (!isMobile) {
    return <PipelinesTable {...list} />
  }

  return (
    <Box style={{ flex: 1, minHeight: 0, overflowY: 'auto' }}>
      <VirtualList
        items={pipelines}
        getKey={getPipelineId}
        estimateSize={104}
        onEndReached={onEndReached}
        renderItem={(pipeline) => (
          <Box pb="xs">
            <MobilePipelineCard
              terms={terms}
              pipeline={pipeline}
              isSelected={selectedPipelines.has(pipeline.id)}
              onToggle={() => dispatch({ type: 'TOGGLE_PIPELINE', pipelineId: pipeline.id })}
            />
          </Box>
        )}
      />
    </Box>
  )
}
