import { Box, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronRight, IconFolder, IconFolders, IconGitBranch } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useContextMenu } from '../../../components/common/ContextMenu'
import { RepositoryFetchIndicator, RowFetchInfo } from '../../../components/common/PipelineFetchInfo'
import { SectionActions, WorkflowActions } from '../../../components/common/PipelineRowActions'
import { PipelineStatusBadge } from '../../../components/common/PipelineStatusBadge'
import { VirtualTable, type VirtualTableColumn } from '../../../components/common/VirtualTable'
import type { PipelineActions, TableRow } from '../../../types/pipelineRows'
import type { ProviderSummary } from '../../../types/providers'
import type { ViewDisplay } from '../../../types/views'
import { historyLabel, rowMenuItems, workflowMenuItems } from '../../../utils/pipelineRowActions'
import { TableCells } from '../../../utils/tableCells'
import { ProviderStatusButton } from '../../notifications/components/ProviderStatusButton'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { accessorSort } from '../lib/pipelineRows'

const ROW_HEIGHT = 48

interface PipelineNameCellProps {
  row: TableRow
  onToggle: (id: string) => void
  actions: PipelineActions
}

export function PipelineNameCell({ row, onToggle, actions }: PipelineNameCellProps): ReactNode {
  const indent = row.depth ? `calc(var(--mantine-spacing-xl) * ${row.depth})` : undefined

  if (row.type === 'section') {
    return (
      <UnstyledButton
        className="item-primary"
        w="100%"
        {...(indent === undefined ? {} : { pl: indent })}
        onClick={() => onToggle(row.id)}
        aria-expanded={row.expanded}
      >
        <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
          {row.expanded ? (
            <IconChevronDown size={16} style={{ flexShrink: 0 }} aria-hidden />
          ) : (
            <IconChevronRight size={16} style={{ flexShrink: 0 }} aria-hidden />
          )}
          {row.kind === 'view' ? (
            <IconFolders size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} aria-hidden />
          ) : (
            <IconFolder size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} aria-hidden />
          )}
          {TableCells.truncated(row.name)}
          {row.kind === 'view' && (
            <Text size="sm" c="dimmed">
              {row.pipelineCount}
            </Text>
          )}
        </Group>
      </UnstyledButton>
    )
  }

  const { pipeline } = row

  return (
    <UnstyledButton
      className="item-primary"
      w="100%"
      {...(indent === undefined ? {} : { pl: indent })}
      onClick={() => actions.onViewHistory(pipeline)}
      aria-label={`View ${historyLabel(actions, pipeline.provider_type)} for ${row.name}`}
    >
      <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
        <IconGitBranch size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} aria-hidden />
        {TableCells.truncated(row.name)}
      </Group>
    </UnstyledButton>
  )
}

interface PipelineColumnsInput {
  groupTitle: string
  sortable: boolean
  getProvider: (id: number) => ProviderSummary | undefined
  actions: PipelineActions
  onToggle: (id: string) => void
}

export function usePipelineColumns({
  groupTitle,
  sortable,
  getProvider,
  actions,
  onToggle,
}: PipelineColumnsInput): VirtualTableColumn<TableRow>[] {
  return [
    {
      id: 'name',
      title: 'Name',
      sortable,
      width: '35%',
      textAlign: 'left',
      render: (row) => <PipelineNameCell row={row} onToggle={onToggle} actions={actions} />,
    },
    {
      id: 'organization',
      title: groupTitle,
      sortable,
      width: 160,
      textAlign: 'left',
      render: (row) => TableCells.truncatedDimmed(row.organization),
    },
    {
      id: 'status',
      title: 'Status',
      sortable,
      width: 120,
      textAlign: 'center',
      render: (row) =>
        row.type === 'workflow' ? (
          <PipelineStatusBadge pipeline={row.pipeline} provider={getProvider(row.pipeline.provider_id)} />
        ) : (
          <RowFetchInfo row={row} getProvider={getProvider} />
        ),
    },
    {
      id: 'provider',
      title: 'Provider',
      sortable,
      width: 200,
      textAlign: 'left',
      render: (row) => (
        <Stack gap="3xs" align="flex-start">
          {Array.from(row.providerIds).map((providerId) => (
            <ProviderStatusButton key={providerId} provider={getProvider(providerId)} />
          ))}
        </Stack>
      ),
    },
    {
      id: 'actions',
      title: 'Actions',
      width: actions.onViewMetrics ? 220 : 200,
      textAlign: 'right',
      render: (row) =>
        row.type === 'section' ? (
          <Group justify="flex-end" wrap="nowrap">
            <RepositoryFetchIndicator row={row} />
            <SectionActions
              name={row.name}
              providerIds={Array.from(row.providerIds)}
              items={rowMenuItems(row, actions)}
              actions={actions}
            />
          </Group>
        ) : (
          <Group justify="flex-end">
            <WorkflowActions pipeline={row.pipeline} actions={actions} />
          </Group>
        ),
    },
  ]
}

interface PipelineRowsTableProps {
  model: PipelineViewPageModel
  actions: PipelineActions
  onDisplayChange: (display: ViewDisplay) => void
}

export function PipelineRowsTable({ model, actions, onDisplayChange }: PipelineRowsTableProps): ReactNode {
  const { rows, terminology, sortable, sortStatus, display, selecting, selectedSet, selectRowKeys, getProvider } = model
  const contextMenu = useContextMenu()
  const columns = usePipelineColumns({
    groupTitle: terminology?.group_title ?? '',
    sortable,
    getProvider,
    actions,
    onToggle: model.toggleSection,
  })
  const selectedKeys = new Set(
    rows.flatMap((row) => (row.type === 'workflow' && selectedSet.has(row.pipeline.id) ? [row.id] : []))
  )

  const tableSelection = selecting
    ? {
        selectedKeys,
        isSelectable: (row: TableRow) => row.type === 'workflow',
        onChange: selectRowKeys,
        label: (row: TableRow) => `Select ${row.name}`,
      }
    : undefined

  return (
    <Box style={{ width: '100%', height: '100%', display: 'flex', flexDirection: 'column', minHeight: 0, flex: 1 }}>
      <VirtualTable<TableRow>
        records={rows}
        columns={columns}
        getKey={(row) => row.id}
        height="100%"
        rowHeight={ROW_HEIGHT}
        label={`${terminology?.items ?? 'pipelines'} table`}
        sort={sortStatus}
        onSortChange={({ columnId, direction }) => {
          const sort = accessorSort(columnId)

          if (display && sort) {
            onDisplayChange({ ...display, sort, direction })
          }
        }}
        onRowClick={(row) =>
          row.type === 'workflow' ? actions.onViewHistory(row.pipeline) : model.toggleSection(row.id)
        }
        rowAttributes={(row) =>
          contextMenu(() =>
            row.type === 'workflow' ? workflowMenuItems(row.pipeline, actions) : rowMenuItems(row, actions)
          )
        }
        selection={tableSelection}
      />
    </Box>
  )
}
