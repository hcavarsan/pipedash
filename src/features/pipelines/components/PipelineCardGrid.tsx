import { Box, Card, Checkbox, Group, px, Stack, Text, UnstyledButton, useMantineTheme } from '@mantine/core'
import { type ReactNode, type RefObject, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useContextMenu } from '../../../components/common/ContextMenu'
import { IosPipelineRow, relativeOrNever, staleFooter } from '../../../components/common/IosPipelineList'
import {
  FetchInfo,
  type PipelineActions,
  type PipelineSelection,
  PipelineStatusBadge,
  WorkflowActionMenu,
  WorkflowActions,
  workflowMenuItems,
  workflowSwipeActions,
} from '../../../components/common/PipelineRowParts'
import { VirtualList } from '../../../components/common/VirtualList'
import { IosGroup } from '../../../components/ios/IosList'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import type { ProviderSummary, ViewPipeline } from '../../../types/domain'
import { formatDateTime } from '../../../utils/formatDateTime'
import { getPipelineLabels, getStatusLabel } from '../../../utils/pipelinePresentation'

const MOBILE_ROW_ESTIMATE = 64
const CARD_ROW_ESTIMATE = 130

interface GridRow {
  key: string
  pipelines: ViewPipeline[]
}

interface PipelineCardGridProps {
  header?: string | undefined
  headerAction?: ReactNode
  pipelines: ViewPipeline[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection | undefined
}

function useColumnCount(containerRef: RefObject<HTMLDivElement | null>): number {
  const { breakpoints } = useMantineTheme()
  const [columns, setColumns] = useState(1)

  useLayoutEffect(() => {
    const container = containerRef.current
    const sm = px(breakpoints.sm)
    const lg = px(breakpoints.lg)
    const xl = px(breakpoints.xl)
    if (!container || typeof sm !== 'number' || typeof lg !== 'number' || typeof xl !== 'number') {
      return
    }
    const measure = () => {
      const width = window.innerWidth
      setColumns(width >= xl ? 4 : width >= lg ? 3 : width >= sm ? 2 : 1)
    }

    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(container)

    return () => observer.disconnect()
  }, [containerRef, breakpoints.sm, breakpoints.lg, breakpoints.xl])

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
}

function IosPipelineItem({
  pipeline,
  getProvider,
  actions,
  selection,
  now,
}: PipelineItemProps & { now: number }): ReactNode {
  const provider = getProvider(pipeline.provider_id)
  const subtitle = [pipeline.repository, pipeline.branch, relativeOrNever(pipeline.last_run, now)]
    .filter(Boolean)
    .join(' · ')

  return (
    <IosPipelineRow
      status={pipeline.status}
      statusText={getStatusLabel(pipeline)}
      title={pipeline.name}
      subtitle={subtitle}
      hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
      longPressMenu
      selected={selection ? selection.selected.has(pipeline.id) : undefined}
      onClick={() => (selection ? selection.onToggle(pipeline) : actions.onViewHistory(pipeline))}
      swipeActions={workflowSwipeActions(pipeline, actions)}
      menu={<WorkflowActionMenu pipeline={pipeline} provider={provider} actions={actions} />}
    />
  )
}

function PipelineCard({ pipeline, getProvider, actions, selection }: PipelineItemProps): ReactNode {
  const contextMenu = useContextMenu()

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
              mt={2}
              aria-label={`Select ${pipeline.name}`}
              checked={selection.selected.has(pipeline.id)}
              onChange={() => selection.onToggle(pipeline)}
            />
          )}
          <UnstyledButton
            className="item-primary"
            onClick={() => actions.onViewHistory(pipeline)}
            aria-label={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()} for ${pipeline.name}`}
            miw={0}
            flex={1}
          >
            <Text size="sm" fw={600} truncate>
              {pipeline.name}
            </Text>
            <Text size="xs" c="dimmed" truncate>
              {pipeline.repository}
            </Text>
          </UnstyledButton>
          <Group gap="xs" wrap="nowrap" style={{ flexShrink: 0 }}>
            <PipelineStatusBadge pipeline={pipeline} provider={getProvider(pipeline.provider_id)} />
            <FetchInfo providers={[getProvider(pipeline.provider_id)].flatMap((provider) => provider ?? [])} />
          </Group>
        </Group>

        <Group justify="space-between" wrap="nowrap" gap="sm">
          <Text size="xs" c="dimmed" truncate>
            {getProvider(pipeline.provider_id)?.name ?? 'Unknown'} ·{' '}
            {pipeline.last_run ? formatDateTime(pipeline.last_run) : 'Never run'}
          </Text>
          <WorkflowActions pipeline={pipeline} actions={actions} />
        </Group>
      </Stack>
    </Card>
  )
}

export function PipelineCardGrid({
  header,
  headerAction,
  pipelines,
  getProvider,
  actions,
  selection,
}: PipelineCardGridProps): ReactNode {
  const mobileShell = useMobileShell()
  const now = useNow(30_000)
  const containerRef = useRef<HTMLDivElement>(null)
  const measuredColumns = useColumnCount(containerRef)
  const columns = mobileShell ? 1 : measuredColumns
  const rows = useMemo(() => chunkPipelines(pipelines, columns), [pipelines, columns])

  const list = (
    <div ref={containerRef}>
      <VirtualList
        items={rows}
        getKey={(row) => row.key}
        estimateSize={mobileShell ? MOBILE_ROW_ESTIMATE : CARD_ROW_ESTIMATE}
        renderItem={(row, index) =>
          mobileShell ? (
            row.pipelines.map((pipeline) => (
              <IosPipelineItem
                key={pipeline.id}
                pipeline={pipeline}
                getProvider={getProvider}
                actions={actions}
                selection={selection}
                now={now}
              />
            ))
          ) : (
            <Box
              pb={index === rows.length - 1 ? 0 : 'sm'}
              style={{
                display: 'grid',
                gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                gap: 'var(--mantine-spacing-sm)',
              }}
            >
              {row.pipelines.map((pipeline) => (
                <PipelineCard
                  key={pipeline.id}
                  pipeline={pipeline}
                  getProvider={getProvider}
                  actions={actions}
                  selection={selection}
                />
              ))}
            </Box>
          )
        }
      />
    </div>
  )

  return mobileShell ? (
    <IosGroup
      header={header}
      headerAction={headerAction}
      footer={staleFooter(
        pipelines.map((pipeline) => pipeline.provider_id),
        getProvider,
        now
      )}
    >
      {list}
    </IosGroup>
  ) : (
    list
  )
}
