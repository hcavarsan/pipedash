import { Box, Card, Checkbox, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconCalendar, IconClock } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { RunDuration } from '../../../components/common/RunDuration'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineRun } from '../../../types/pipelines'
import { formatDateTime } from '../../../utils/formatDateTime'
import { getRunStatusLabel } from '../../../utils/pipelinePresentation'
import { TableCells } from '../../../utils/tableCells'

import { RunActionButtons } from './RunActionControls'
import { useRunActions } from './RunActionsContext'

interface RunFieldProps {
  icon?: ReactNode
  label: string
  children: ReactNode
}

export function RunField({ icon, label, children }: RunFieldProps): ReactNode {
  return (
    <Box flex={1} miw={0}>
      <Group gap="xs" wrap="nowrap" c="dimmed">
        {icon}
        <Text size={TYPOGRAPHY.FIELD_LABEL.size}>{label}</Text>
      </Group>
      {children}
    </Box>
  )
}

interface RunHistoryCardProps {
  run: PipelineRun
  selected: boolean
  selectable: boolean
  onSelectedChange: (checked: boolean) => void
}

function RunCardHeader({ run, selected, selectable, onSelectedChange }: RunHistoryCardProps): ReactNode {
  const { labels, openRun } = useRunActions()

  return (
    <Group justify="space-between" wrap="nowrap">
      <Group gap="xs" wrap="nowrap" flex={1} miw={0}>
        {selectable && (
          <Checkbox
            aria-label={`Select run #${run.run_number}`}
            checked={selected}
            onChange={(event) => onSelectedChange(event.currentTarget.checked)}
          />
        )}
        <UnstyledButton
          className="item-primary"
          aria-label={`View details for ${labels.run} #${run.run_number}`}
          onClick={() => openRun(run)}
        >
          <Group gap="xs" wrap="nowrap">
            <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight}>
              {run.display.title}
            </Text>
            <StatusBadge status={run.status} label={getRunStatusLabel(run)} />
          </Group>
        </UnstyledButton>
      </Group>
      <RunActionButtons run={run} withDetails={false} />
    </Group>
  )
}

const ContextField = ({ label, value }: { label: string; value: string }): ReactNode => (
  <RunField label={label}>
    <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
      {value}
    </Text>
  </RunField>
)

function RunCardContext({ run }: { run: PipelineRun }): ReactNode {
  const { context } = run.display

  return (
    context.length > 0 && (
      <Group gap="lg" wrap="nowrap" align="flex-start">
        {context.map((field) => (
          <ContextField key={field.label} {...field} />
        ))}
      </Group>
    )
  )
}

function RunCardTiming({ run }: { run: PipelineRun }): ReactNode {
  return (
    <Group gap="lg" wrap="nowrap" align="flex-start">
      <RunField icon={<IconClock size={14} aria-hidden />} label="Duration">
        <Text size={TYPOGRAPHY.FIELD_VALUE.size}>
          <RunDuration run={run} />
        </Text>
      </RunField>
      <RunField icon={<IconCalendar size={14} aria-hidden />} label={run.display.started_label}>
        <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
          {run.started_at ? formatDateTime(run.started_at) : '—'}
        </Text>
      </RunField>
    </Group>
  )
}

export function RunHistoryCard(props: RunHistoryCardProps): ReactNode {
  const { run } = props

  return (
    <Card padding="lg" className="interactive-surface stretched">
      <Stack gap="xs">
        <RunCardHeader {...props} />
        <RunCardContext run={run} />
        <RunCardTiming run={run} />
        {run.commit_sha && <RunField label="Commit">{TableCells.commit(run.commit_sha)}</RunField>}
      </Stack>
    </Card>
  )
}
