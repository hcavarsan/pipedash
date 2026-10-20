import { Alert, Box, Button, Code, Divider, Group, Paper, Stack, Text } from '@mantine/core'
import { IconExternalLink, IconPlayerPlay, IconRefresh, IconSquare } from '@tabler/icons-react'
import { Fragment, useEffect, useMemo, useRef, useState } from 'react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import { useRerunWorkflow, useRunDetails } from '../../queries/useRunDetailsQuery'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import { service } from '../../services'
import { STATUS_META } from '../../theme/status'
import { TYPOGRAPHY } from '../../theme/typography'
import type { CellRenderer, PipelineStatus } from '../../types'
import { filterVisibleColumns } from '../../utils/columnBuilder'
import { type CellValue, renderCellValue } from '../../utils/dynamicRenderers'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { getValueByPath } from '../../utils/objectPath'
import { CopyButton } from '../atoms/CopyButton'
import { StatusBadge } from '../atoms/StatusBadge'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'

interface WorkflowLogsModalProps {
  opened: boolean
  onClose: () => void
  pipelineId: string
  runNumber: number
  providerId?: number
  onRerunSuccess?: (pipelineId: string, newRunNumber: number) => void
  onCancelSuccess?: () => void
}

interface DetailField {
  id: string
  label: string
  field_path: string
  renderer: CellRenderer
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

export const WorkflowLogsModal = ({
  opened,
  onClose,
  pipelineId,
  runNumber,
  providerId,
  onRerunSuccess,
  onCancelSuccess,
}: WorkflowLogsModalProps) => {
  const [cancelling, setCancelling] = useState(false)
  const { isMobile } = useIsMobile()
  const abortRef = useRef(false)

  useEffect(() => {
    abortRef.current = false

    return () => {
      abortRef.current = true
    }
  }, [])

  const {
    data: runDetails,
    isLoading: loading,
    isFetching,
    error: queryError,
    refetch,
  } = useRunDetails(pipelineId, runNumber, opened)

  const rerunMutation = useRerunWorkflow()

  const { data: tableSchema } = useTableDefinition(providerId ?? 0, 'pipeline_runs')

  const fields = useMemo<DetailField[]>(() => {
    const columns = tableSchema ? filterVisibleColumns(tableSchema.columns).filter((col) => col.id !== 'actions') : []

    return columns.length > 0 ? columns : FALLBACK_FIELDS
  }, [tableSchema])

  const isRunning = runDetails?.status === 'running' || runDetails?.status === 'pending'
  const isPending = runDetails?.status === 'pending'

  const handleRerun = async () => {
    if (!runDetails) {
      return
    }

    try {
      const result = await rerunMutation.mutateAsync({
        pipelineId,
        inputs: runDetails.inputs,
      })

      onClose()

      if (onRerunSuccess && result.newRunNumber > 0) {
        onRerunSuccess(pipelineId, result.newRunNumber)
      }
    } catch (error: unknown) {
      logger.error('WorkflowLogsModal', 'Failed to re-run workflow', error)
    }
  }

  const handleCancel = async () => {
    if (!runDetails) {
      return
    }

    setCancelling(true)

    try {
      await service.cancelPipelineRun(pipelineId, runNumber)

      const maxAttempts = 10
      let attempt = 0
      let statusChanged = false

      while (attempt < maxAttempts && !statusChanged && !abortRef.current) {
        const { promise: delay, resolve: elapsed } = Promise.withResolvers<void>()

        setTimeout(elapsed, 1000)
        await delay

        if (abortRef.current) {
          break
        }

        try {
          const { data: freshDetails } = await refetch()

          if (freshDetails && freshDetails.status === 'cancelled') {
            statusChanged = true
          }
        } catch {
          /* refetch may fail during polling */
        }

        attempt++
      }

      if (abortRef.current) {
        return
      }

      if (!statusChanged) {
        await refetch()
      }

      if (onCancelSuccess) {
        onCancelSuccess()
      }
    } catch (error: unknown) {
      if (abortRef.current) {
        return
      }

      displayErrorNotification(error, `Couldn't cancel run #${runNumber}`)
    } finally {
      if (!abortRef.current) {
        setCancelling(false)
      }
    }
  }

  const renderValue = (field: DetailField, value: unknown) => {
    if (field.id === 'status' && isPipelineStatus(value)) {
      return <StatusBadge status={value} size="md" withIcon />
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

    return renderCellValue(field.renderer, value as CellValue)
  }

  const footer = runDetails ? (
    <Stack gap="xs">
      {isPending && (
        <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} c="dimmed">
          This run is pending. It can be stopped or rerun once it starts.
        </Text>
      )}
      <Group justify="flex-end" gap="sm" grow={isMobile}>
        <Button
          component="a"
          href={runDetails.logs_url}
          target="_blank"
          rel="noopener noreferrer"
          variant="default"
          size="sm"
          rightSection={<IconExternalLink size={16} />}
        >
          {isMobile ? 'Logs' : 'View full logs'}
        </Button>
        {isRunning ? (
          <Button
            color="red"
            size="sm"
            leftSection={<IconSquare size={16} />}
            onClick={handleCancel}
            loading={cancelling}
            disabled={isPending}
          >
            {isMobile ? 'Stop' : 'Stop workflow'}
          </Button>
        ) : (
          <Button
            size="sm"
            leftSection={<IconPlayerPlay size={16} />}
            onClick={handleRerun}
            loading={rerunMutation.isPending}
          >
            {isMobile ? 'Rerun' : 'Rerun workflow'}
          </Button>
        )}
      </Group>
    </Stack>
  ) : null

  const renderContent = () => {
    if (queryError) {
      return (
        <ErrorState
          title={`Couldn't load run #${runNumber}`}
          error={queryError}
          onRetry={() => refetch()}
          retrying={isFetching}
        />
      )
    }

    if (loading || !runDetails) {
      return <LoadingState message="Loading run details..." />
    }

    const rows = fields.flatMap((field) => {
      const value = getValueByPath(runDetails, field.field_path)

      if ((value === null || value === undefined) && field.id !== 'status' && field.id !== 'run_number') {
        return []
      }

      return [{ field, value }]
    })

    return (
      <Stack gap={isMobile ? 'xs' : 'md'}>
        {isRunning && (
          <Alert color="blue" icon={<IconRefresh size={16} />} title="Run in progress">
            <Text size={TYPOGRAPHY.HELPER_TEXT.size}>Details refresh automatically every 10 seconds.</Text>
          </Alert>
        )}

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
                      {renderValue(field, value)}
                    </Group>
                  </Box>
                </Group>
              </Fragment>
            ))}
          </Stack>
        </Paper>
      </Stack>
    )
  }

  return (
    <StandardModal opened={opened} onClose={onClose} size="lg" title="Run details" footer={footer}>
      {renderContent()}
    </StandardModal>
  )
}
