import { Box, Card, Checkbox, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { type ReactNode, type RefObject, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { StatusIcon } from '../../../components/atoms/StatusBadge'
import { useContextMenu } from '../../../components/common/ContextMenu'
import { FetchInfo } from '../../../components/common/PipelineFetchInfo'
import { WorkflowActions } from '../../../components/common/PipelineRowActions'
import { PipelineStatusBadge } from '../../../components/common/PipelineStatusBadge'
import { VirtualList } from '../../../components/common/VirtualList'
import { SPACING_PX } from '../../../theme/theme'
import type { PipelineActions, PipelineSelection } from '../../../types/pipelineRows'
import type { ProviderSummary } from '../../../types/providers'
import type { ViewPipeline } from '../../../types/views'
import { getPipelineStatusLabel, pipelineStatusKey } from '../../../utils/pipelinePresentation'
import { historyLabel, workflowMenuItems } from '../../../utils/pipelineRowActions'
import { PipelineCardMeta } from './PipelineCardMeta'

const CARD_ROW_ESTIMATE = 130
const CARD_MIN_WIDTH = 280
const MAX_COLUMNS = 4

interface GridRow {
  key: string
  pipelines: ViewPipeline[]
}

interface PipelineCardGridProps {
  pipelines: ViewPipeline[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection | undefined
  pinned?: boolean | undefined
}

function useColumnCount(containerRef: RefObject<HTMLDivElement | null>): number {
  const [columns, setColumns] = useState(1)

  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) {
      return
    }
    const measure = () => {
      const gap = SPACING_PX.sm
      const fitting = Math.floor((container.clientWidth + gap) / (CARD_MIN_WIDTH + gap))

      setColumns(Math.min(MAX_COLUMNS, Math.max(1, fitting)))
    }

    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(container)

    return () => observer.disconnect()
  }, [containerRef])

  return columns
}

function chunkPipelines(pipelines: ViewPipeline[], columns: number): GridRow[] {
  const rows: GridRow[] = []
  for (let start = 0; start < pipelines.length; start += columns) {
    const chunk = pipelines.slice(start, start + columns)
    const [first] = chunk
    if (first) {
      rows.push({ key: first.id, pipelines: chunk })
    }
  }

  return rows
}

interface PipelineItemProps {
  pipeline: ViewPipeline
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection | undefined
  pinned?: boolean | undefined
}

function PipelineCard({ pipeline, getProvider, actions, selection, pinned }: PipelineItemProps): ReactNode {
  const contextMenu = useContextMenu()
  const provider = getProvider(pipeline.provider_id)

  return (
    <Card
      padding="sm"
      className="interactive-surface stretched"
      {...contextMenu(() => workflowMenuItems(pipeline, actions))}
    >
      <Stack gap="xs" h="100%" justify="space-between">
        <Group justify="space-between" align="flex-start" wrap="nowrap" gap="sm">
          {selection && (
            <Checkbox
              mt="4xs"
              aria-label={`Select ${pipeline.name}`}
              checked={selection.selected.has(pipeline.id)}
              onChange={() => selection.onToggle(pipeline)}
            />
          )}
          <UnstyledButton
            className="item-primary"
            onClick={() => actions.onViewHistory(pipeline)}
            aria-label={`View ${historyLabel(actions, pipeline.provider_type)} for ${pipeline.name}`}
            miw={0}
            flex={1}
          >
            <Text size="sm" fw={600} truncate>
              {pipeline.name}
            </Text>
            <PipelineCardMeta pipeline={pipeline} provider={provider} />
          </UnstyledButton>
          <Group gap="xs" wrap="nowrap" style={{ flexShrink: 0 }}>
            {pinned ? (
              <StatusIcon status={pipelineStatusKey(pipeline)} label={getPipelineStatusLabel(pipeline)} />
            ) : (
              <PipelineStatusBadge pipeline={pipeline} provider={provider} />
            )}
            <FetchInfo providers={provider ? [provider] : []} />
          </Group>
        </Group>

        <Group justify="flex-end" wrap="nowrap" gap="sm">
          <WorkflowActions pipeline={pipeline} actions={actions} />
        </Group>
      </Stack>
    </Card>
  )
}

function GridCards({ row, ...rest }: { row: GridRow } & Omit<PipelineCardGridProps, 'pipelines'>): ReactNode {
  return row.pipelines.map((pipeline) => <PipelineCard key={pipeline.id} pipeline={pipeline} {...rest} />)
}

export function PipelineCardGrid({
  pipelines,
  getProvider,
  actions,
  selection,
  pinned,
}: PipelineCardGridProps): ReactNode {
  const containerRef = useRef<HTMLDivElement>(null)
  const columns = useColumnCount(containerRef)
  const rows = useMemo(() => chunkPipelines(pipelines, columns), [pipelines, columns])

  return (
    <div ref={containerRef}>
      <VirtualList
        items={rows}
        getKey={(row) => row.key}
        estimateSize={CARD_ROW_ESTIMATE}
        renderItem={(row, index) => (
          <Box
            pb={index === rows.length - 1 ? 0 : 'sm'}
            style={{
              display: 'grid',
              gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
              gap: 'var(--mantine-spacing-sm)',
            }}
          >
            <GridCards row={row} getProvider={getProvider} actions={actions} selection={selection} pinned={pinned} />
          </Box>
        )}
      />
    </div>
  )
}
