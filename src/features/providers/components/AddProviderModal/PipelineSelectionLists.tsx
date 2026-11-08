import { Box, Group, Checkbox as MantineCheckbox, Stack, Text, Tooltip } from '@mantine/core'
import type { Dispatch, ReactNode } from 'react'
import { VirtualList } from '../../../../components/common/VirtualList'
import { VirtualTable, type VirtualTableColumn } from '../../../../components/common/VirtualTable'
import shell from '../../../../components/forms/ShellForm.module.css'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { TYPOGRAPHY } from '../../../../theme/typography'
import type { AvailablePipeline } from '../../../../types/pipelines'
import type { PipelineTerminology } from '../../../../types/presentation'
import { CheckCircle } from './ShellForm'
import type { FormAction } from './types'

export interface PipelineListProps {
  pipelines: AvailablePipeline[]
  selectedPipelines: Set<string>
  dispatch: Dispatch<FormAction>
  terms: PipelineTerminology
  hasMore: boolean
  onSelectAll: () => void
  onEndReached: () => void
}

export const getPipelineId = (pipeline: AvailablePipeline): string => pipeline.id

function toggleSelection(
  { pipelines, selectedPipelines, dispatch }: Pick<PipelineListProps, 'pipelines' | 'selectedPipelines' | 'dispatch'>,
  next: Set<string>
): void {
  const visible = pipelines.map(getPipelineId)
  const added = visible.filter((id) => next.has(id) && !selectedPipelines.has(id))
  const removed = visible.filter((id) => !next.has(id) && selectedPipelines.has(id))

  if (added.length > 0) {
    dispatch({ type: 'SELECT_ALL_PIPELINES', pipelineIds: added })
  }

  if (removed.length > 0) {
    dispatch({ type: 'DESELECT_PIPELINES', pipelineIds: removed })
  }
}

interface MobilePipelineCardProps {
  pipeline: AvailablePipeline
  isSelected: boolean
  onToggle: () => void
  terms: PipelineTerminology
}

export function MobilePipelineCard({ pipeline, isSelected, onToggle, terms }: MobilePipelineCardProps): ReactNode {
  return (
    <MantineCheckbox.Card checked={isSelected} onClick={onToggle} p="xs" aria-label={pipeline.name}>
      <Stack gap="3xs">
        <Group gap="xs" wrap="nowrap">
          <MantineCheckbox.Indicator />
          <Text size="sm" fw={500} truncate style={{ flex: 1 }}>
            {pipeline.name}
          </Text>
        </Group>

        <Group gap="xs" wrap="nowrap" align="flex-start">
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
              {terms.group_title}
            </Text>
            <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} truncate>
              {pipeline.organization || '—'}
            </Text>
          </Box>
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
              {terms.source_title}
            </Text>
            <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} truncate>
              {pipeline.repository || '—'}
            </Text>
          </Box>
        </Group>

        {pipeline.description && (
          <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} c="dimmed" lineClamp={1}>
            {pipeline.description}
          </Text>
        )}
      </Stack>
    </MantineCheckbox.Card>
  )
}

const TABLE_FIELDS = ['name', 'organization', 'repository', 'description'] as const

export function PipelinesTable({
  pipelines,
  selectedPipelines,
  dispatch,
  terms,
  hasMore,
  onSelectAll,
  onEndReached,
}: PipelineListProps): ReactNode {
  const titles = {
    name: 'Name',
    organization: terms.group_title,
    repository: terms.source_title,
    description: 'Description',
  }
  const widths = { name: '35%', organization: '20%', repository: '20%', description: '25%' }
  const columns: VirtualTableColumn<AvailablePipeline>[] = TABLE_FIELDS.map((key) => ({
    id: key,
    title: titles[key],
    width: widths[key],
    render: (pipeline) => (
      <Tooltip label={pipeline[key] || '—'} openDelay={500}>
        <Text
          size="sm"
          {...(key === 'name' ? { fw: 500 } : {})}
          {...(key === 'description' ? { c: 'dimmed' } : {})}
          truncate="end"
        >
          {pipeline[key] || '—'}
        </Text>
      </Tooltip>
    ),
  }))

  return (
    <Box style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
      <VirtualTable
        records={pipelines}
        columns={columns}
        getKey={getPipelineId}
        height="fill"
        rowHeight={44}
        label={`Available ${terms.items}`}
        onEndReached={onEndReached}
        selection={{
          selectedKeys: selectedPipelines,
          isSelectable: () => true,
          onChange: (next) => toggleSelection({ pipelines, selectedPipelines, dispatch }, next),
          label: (pipeline) => pipeline.name,
          hasMore,
          onToggleAll: onSelectAll,
        }}
      />
    </Box>
  )
}

function PipelineSubtitle({ source, description }: { source: string; description: string | null }): ReactNode {
  return (
    <>
      {source && <span className={shell.clamp}>{source}</span>}
      {description && <span className={shell.clamp}>{description}</span>}
    </>
  )
}

export function ShellPipelineList({
  pipelines,
  selectedPipelines,
  dispatch,
  terms,
  hasMore,
  onSelectAll,
  onEndReached,
}: PipelineListProps): ReactNode {
  const allSelected = !hasMore && pipelines.every((pipeline) => selectedPipelines.has(pipeline.id))

  return (
    <IosGroup
      header={selectedPipelines.size > 0 ? `${selectedPipelines.size} selected` : terms.items}
      headerAction={
        <button type="button" className={shell.headerAction} onClick={onSelectAll}>
          {allSelected ? 'Deselect all' : 'Select all'}
        </button>
      }
    >
      <VirtualList
        items={pipelines}
        getKey={getPipelineId}
        estimateSize={64}
        onEndReached={onEndReached}
        renderItem={(pipeline) => {
          const selected = selectedPipelines.has(pipeline.id)
          const source = [pipeline.organization, pipeline.repository].filter(Boolean).join(' · ')

          return (
            <IosRow
              title={pipeline.name}
              subtitle={<PipelineSubtitle source={source} description={pipeline.description} />}
              accessory={<CheckCircle checked={selected} />}
              selected={selected}
              onClick={() => dispatch({ type: 'TOGGLE_PIPELINE', pipelineId: pipeline.id })}
              ariaLabel={pipeline.name}
            />
          )
        }}
      />
    </IosGroup>
  )
}
