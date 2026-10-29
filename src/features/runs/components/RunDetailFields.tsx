import { Box, Code, Divider, Group, Paper, Stack, Text } from '@mantine/core'
import { Fragment, type ReactNode, useMemo } from 'react'
import { CopyButton } from '../../../components/atoms/CopyButton'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { STATUS_META } from '../../../theme/status'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { CellRenderer, PipelineRun, PipelineStatus } from '../../../types/domain'
import { getStatusLabel } from '../../../utils/pipelinePresentation'
import { useTableDefinition } from '../queries/useTableSchemaQueries'
import { filterVisibleColumns } from '../utils/columnBuilder'
import { isCellValue, renderCellValue } from '../utils/dynamicRenderers'
import { getValueByPath } from '../utils/objectPath'
import classes from './WorkflowLogsModal.module.css'

interface DetailField {
  id: string
  label: string
  field_path: string
  renderer: CellRenderer
}

interface DetailRow {
  field: DetailField
  value: unknown
}

const FALLBACK_FIELDS: DetailField[] = [
  { id: 'run_number', label: 'Run number', field_path: 'run_number', renderer: 'Text' },
  { id: 'status', label: 'Status', field_path: 'status', renderer: 'StatusBadge' },
  { id: 'branch', label: 'Branch', field_path: 'branch', renderer: 'Text' },
  { id: 'duration_seconds', label: 'Duration', field_path: 'duration_seconds', renderer: 'Duration' },
  { id: 'started_at', label: 'Started at', field_path: 'started_at', renderer: 'DateTime' },
  { id: 'concluded_at', label: 'Concluded at', field_path: 'concluded_at', renderer: 'DateTime' },
  { id: 'actor', label: 'Actor', field_path: 'actor', renderer: 'Avatar' },
]

const isPipelineStatus = (value: unknown): value is PipelineStatus => typeof value === 'string' && value in STATUS_META

export function useDetailRows(providerId: number | undefined, run: PipelineRun): DetailRow[] {
  const { data: tableSchema } = useTableDefinition(providerId ?? 0, 'pipeline_runs')
  const fields = useMemo<DetailField[]>(() => {
    const columns = tableSchema ? filterVisibleColumns(tableSchema.columns).filter((col) => col.id !== 'actions') : []

    return columns.length > 0 ? columns : FALLBACK_FIELDS
  }, [tableSchema])

  return fields.flatMap((field) => {
    const value = getValueByPath(run, field.field_path)

    if ((value === null || value === undefined) && field.id !== 'status' && field.id !== 'run_number') {
      return []
    }

    return [{ field, value }]
  })
}

function RunDetailValue({ field, value, run }: { field: DetailField; value: unknown; run: PipelineRun }): ReactNode {
  if (field.id === 'status' && isPipelineStatus(value)) {
    return <StatusBadge status={value} label={getStatusLabel(run)} size="md" />
  }

  if (field.id === 'message') {
    return (
      <Text size="sm" style={{ overflowWrap: 'anywhere' }}>
        {String(value)}
      </Text>
    )
  }

  if (field.id === 'run_number') {
    return (
      <Group gap="xs" wrap="nowrap">
        <Text size={TYPOGRAPHY.FIELD_VALUE.size}>#{String(value)}</Text>
        <CopyButton value={String(value)} label="Copy run number" />
      </Group>
    )
  }

  if ((field.id === 'id' || field.id === 'commit_sha') && typeof value === 'string') {
    const isCommit = field.id === 'commit_sha'

    return (
      <Group gap="xs" wrap="nowrap">
        <Code fz={TYPOGRAPHY.FIELD_VALUE_SMALL.size} style={{ wordBreak: 'break-all' }}>
          {isCommit ? value.substring(0, 7) : value}
        </Code>
        <CopyButton value={value} label={isCommit ? 'Copy commit SHA' : 'Copy run ID'} />
      </Group>
    )
  }

  return renderCellValue(field.renderer, isCellValue(value) ? value : undefined)
}

interface RunDetailFieldsProps {
  rows: DetailRow[]
  run: PipelineRun
  mobileShell: boolean
  isMobile: boolean
}

export function RunDetailFields({ rows, run, mobileShell, isMobile }: RunDetailFieldsProps): ReactNode {
  if (mobileShell) {
    return (
      <div data-selectable>
        <IosGroup>
          {rows.map(({ field, value }) =>
            field.id === 'message' ||
            ((field.renderer === 'Text' || field.renderer === 'TruncatedText') &&
              typeof value === 'string' &&
              value.length > 32) ? (
              // Long text wraps under its label; a truncated value would hide the rest on touch screens.
              <IosRow
                key={field.id}
                title={
                  field.renderer === 'TruncatedText' ? (
                    String(value)
                  ) : (
                    <RunDetailValue field={field} value={value} run={run} />
                  )
                }
                subtitle={field.label}
              />
            ) : (
              <IosRow
                key={field.id}
                title={field.label}
                value={
                  <span className={classes.value}>
                    <RunDetailValue field={field} value={value} run={run} />
                  </span>
                }
              />
            )
          )}
        </IosGroup>
      </div>
    )
  }

  return (
    <Paper withBorder p={isMobile ? 'md' : 'lg'}>
      <Stack gap="sm">
        {rows.map(({ field, value }, index) => (
          <Fragment key={field.id}>
            {index > 0 && <Divider />}
            <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
              <Text size={TYPOGRAPHY.FIELD_VALUE.size} c="dimmed" miw={isMobile ? 90 : 130}>
                {field.label}
              </Text>
              <Box flex={1} miw={0}>
                <Group justify="flex-end" ta="right">
                  <RunDetailValue field={field} value={value} run={run} />
                </Group>
              </Box>
            </Group>
          </Fragment>
        ))}
      </Stack>
    </Paper>
  )
}
