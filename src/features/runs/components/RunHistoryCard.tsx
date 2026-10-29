import { Box, Card, Checkbox, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconCalendar, IconClock, IconGitBranch, IconUser } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { RunDuration } from '../../../components/common/RunDuration'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { PipelineRun } from '../../../types/domain'
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

export function RunHistoryCard({ run, selected, selectable, onSelectedChange }: RunHistoryCardProps): ReactNode {
  const { isFlux, labels, openRun, pipeline } = useRunActions()

  return (
    <Card padding="lg" className="interactive-surface stretched">
      <Stack gap="xs">
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
                  {isFlux ? String(run.metadata?.run_label ?? 'Reconciliation') : `#${run.run_number}`}
                </Text>
                <StatusBadge status={run.status} label={getRunStatusLabel(run)} />
              </Group>
            </UnstyledButton>
          </Group>
          <RunActionButtons run={run} withDetails={false} />
        </Group>

        {isFlux ? (
          <Text size="sm" c="dimmed">
            {String(run.metadata?.message ?? run.commit_message ?? '')}
          </Text>
        ) : (
          <Group gap="lg" wrap="nowrap" align="flex-start">
            <RunField icon={<IconGitBranch size={14} aria-hidden />} label="Branch">
              <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
                {run.branch || '—'}
              </Text>
            </RunField>
            <RunField icon={<IconUser size={14} aria-hidden />} label="Actor">
              <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
                {run.actor || '—'}
              </Text>
            </RunField>
          </Group>
        )}

        <Group gap="lg" wrap="nowrap" align="flex-start">
          <RunField icon={<IconClock size={14} aria-hidden />} label="Duration">
            <Text size={TYPOGRAPHY.FIELD_VALUE.size}>
              <RunDuration run={run} typicalSeconds={pipeline?.typical_duration_seconds} />
            </Text>
          </RunField>
          <RunField icon={<IconCalendar size={14} aria-hidden />} label={isFlux ? 'Recorded' : 'Started'}>
            <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
              {run.started_at ? formatDateTime(run.started_at) : '—'}
            </Text>
          </RunField>
        </Group>

        {run.commit_sha && <RunField label="Commit">{TableCells.commit(run.commit_sha)}</RunField>}
      </Stack>
    </Card>
  )
}
