import { Box, Card, Checkbox, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronRight, IconFolder, IconFolders, IconGitBranch } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useContextMenu } from '../../../components/common/ContextMenu'
import { RepositoryFetchIndicator, RowFetchInfo } from '../../../components/common/PipelineFetchInfo'
import { SectionActions, WorkflowActions } from '../../../components/common/PipelineRowActions'
import { PipelineStatusBadge } from '../../../components/common/PipelineStatusBadge'
import { VirtualList } from '../../../components/common/VirtualList'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineActions, PipelineSelection, TableRow } from '../../../types/pipelineRows'
import type { ProviderSummary } from '../../../types/providers'
import { historyLabel, rowMenuItems, workflowMenuItems } from '../../../utils/pipelineRowActions'
import { ProviderStatusButton } from '../../notifications/components/ProviderStatusButton'
import { PipelineCardMeta } from './PipelineCardMeta'

type SectionTableRow = Extract<TableRow, { type: 'section' }>
type WorkflowTableRow = Extract<TableRow, { type: 'workflow' }>

const ROW_ESTIMATE = 150

interface MobilePipelineCardsProps {
  rows: TableRow[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  actions: PipelineActions
  selection?: PipelineSelection | undefined
}

function Field({ label, children }: { label: string; children: ReactNode }): ReactNode {
  return (
    <Box style={{ flex: '1 1 auto' }}>
      <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
        {label}
      </Text>
      <Text size={TYPOGRAPHY.FIELD_VALUE.size}>{children}</Text>
    </Box>
  )
}

interface ProviderButtonsProps {
  row: TableRow
  getProvider: (providerId: number) => ProviderSummary | undefined
}

function ProviderButtons({ row, getProvider }: ProviderButtonsProps): ReactNode {
  return (
    <Group gap="xs" px="xs" wrap="wrap">
      {Array.from(row.providerIds).map((providerId) => (
        <ProviderStatusButton key={providerId} provider={getProvider(providerId)} />
      ))}
    </Group>
  )
}

interface SectionCardProps {
  row: SectionTableRow
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  actions: PipelineActions
}

function SectionSummary({ row }: { row: SectionTableRow }): ReactNode {
  const Chevron = row.expanded ? IconChevronDown : IconChevronRight
  const Folder = row.kind === 'view' ? IconFolders : IconFolder

  return (
    <Stack gap="xs">
      <Group justify="space-between" wrap="nowrap">
        <Group gap="xs" wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
          <Chevron size={16} style={{ flexShrink: 0 }} aria-hidden />
          <Folder size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} aria-hidden />
          <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight} truncate>
            {row.name}
          </Text>
        </Group>
      </Group>

      <Group gap="md" wrap="wrap">
        {row.kind !== 'view' && <Field label={row.organizationLabel}>{row.organization}</Field>}
        <Field label={row.countLabel}>{row.pipelineCount}</Field>
      </Group>
    </Stack>
  )
}

function SectionCard({ row, getProvider, onToggleRepo, actions }: SectionCardProps): ReactNode {
  const contextMenu = useContextMenu()

  return (
    <Card padding="xs" className="interactive-surface stretched" {...contextMenu(() => rowMenuItems(row, actions))}>
      <Stack gap="3xs">
        <Group wrap="nowrap" align="flex-start" gap="xs">
          <UnstyledButton
            className="item-primary"
            onClick={() => onToggleRepo(row.id)}
            aria-expanded={row.expanded}
            p="xs"
            bdrs="sm"
            style={{ flex: 1, minWidth: 0 }}
          >
            <SectionSummary row={row} />
          </UnstyledButton>
          <RowFetchInfo row={row} getProvider={getProvider} />
          <RepositoryFetchIndicator row={row} />
          <SectionActions
            name={row.name}
            providerIds={Array.from(row.providerIds)}
            items={rowMenuItems(row, actions)}
            actions={actions}
          />
        </Group>
        <ProviderButtons row={row} getProvider={getProvider} />
      </Stack>
    </Card>
  )
}

interface WorkflowCardProps {
  row: WorkflowTableRow
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection | undefined
}

function WorkflowSummary({ row, getProvider }: Pick<WorkflowCardProps, 'row' | 'getProvider'>): ReactNode {
  const { pipeline } = row
  const provider = getProvider(pipeline.provider_id)

  return (
    <Stack gap="xs">
      <Group justify="space-between" wrap="nowrap">
        <Group gap="xs" wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
          <IconGitBranch size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} aria-hidden />
          <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight} truncate>
            {row.name}
          </Text>
        </Group>
        <PipelineStatusBadge pipeline={pipeline} provider={provider} />
      </Group>

      <PipelineCardMeta pipeline={pipeline} provider={provider} />
    </Stack>
  )
}

function SelectBox({
  pipeline,
  selection,
}: {
  pipeline: WorkflowTableRow['pipeline']
  selection: PipelineSelection | undefined
}): ReactNode {
  return (
    selection && (
      <Checkbox
        aria-label={`Select ${pipeline.name}`}
        checked={selection.selected.has(pipeline.id)}
        onChange={() => selection.onToggle(pipeline)}
      />
    )
  )
}

function WorkflowCard({ row, getProvider, actions, selection }: WorkflowCardProps): ReactNode {
  const { pipeline } = row
  const contextMenu = useContextMenu()
  const indent = row.depth ? `calc(var(--mantine-spacing-xl) * ${row.depth})` : undefined

  return (
    <Card
      padding="xs"
      {...(indent === undefined ? {} : { ml: indent })}
      className="interactive-surface stretched"
      {...contextMenu(() => workflowMenuItems(pipeline, actions))}
    >
      <Stack gap="xs">
        <UnstyledButton
          className="item-primary"
          onClick={() => actions.onViewHistory(pipeline)}
          aria-label={`View ${historyLabel(actions, pipeline.provider_type)} for ${row.name}`}
          p="xs"
          bdrs="sm"
        >
          <WorkflowSummary row={row} getProvider={getProvider} />
        </UnstyledButton>
        <Group justify="space-between" wrap="nowrap">
          <Group gap="xs" wrap="nowrap">
            <SelectBox pipeline={pipeline} selection={selection} />
            <ProviderButtons row={row} getProvider={getProvider} />
          </Group>
          <WorkflowActions pipeline={pipeline} actions={actions} />
        </Group>
      </Stack>
    </Card>
  )
}

export function MobilePipelineCards({
  rows,
  getProvider,
  onToggleRepo,
  actions,
  selection,
}: MobilePipelineCardsProps): ReactNode {
  return (
    <VirtualList
      items={rows}
      getKey={(row) => row.id}
      estimateSize={ROW_ESTIMATE}
      renderItem={(row, index) => (
        <Box pb={index === rows.length - 1 ? 0 : 'sm'}>
          {row.type === 'section' ? (
            <SectionCard row={row} getProvider={getProvider} onToggleRepo={onToggleRepo} actions={actions} />
          ) : (
            <WorkflowCard row={row} getProvider={getProvider} actions={actions} selection={selection} />
          )}
        </Box>
      )}
    />
  )
}
