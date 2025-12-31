import { useEffect, useMemo, useRef, useState } from 'react'

import { Alert, Avatar, Box, Button, Code, Group, Loader, Paper, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconExternalLink, IconPlayerPlay, IconRefresh, IconSquare } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useRerunWorkflow, useRunDetails } from '../../queries/useRunDetailsQuery'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import { service } from '../../services'
import type { PipelineStatus } from '../../types'
import { filterVisibleColumns } from '../../utils/columnBuilder'
import { DynamicRenderers, THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { formatDuration } from '../../utils/formatDuration'
import { getValueByPath } from '../../utils/objectPath'
import { CopyButton } from '../atoms/CopyButton'
import { StandardModal } from '../common/StandardModal'
import { StatusBadge } from '../common/StatusBadge'

interface WorkflowLogsModalProps {
  opened: boolean;
  onClose: () => void;
  pipelineId: string;
  runNumber: number;
  providerId?: number;
  onRerunSuccess?: (pipelineId: string, newRunNumber: number) => void;
  onCancelSuccess?: () => void;
}


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
    return () => {
      abortRef.current = true
    }
  }, [])

  const {
    data: runDetails,
    isLoading: loading,
    error: queryError,
    refetch,
  } = useRunDetails(pipelineId, runNumber, opened)

  const rerunMutation = useRerunWorkflow()

  const { data: tableSchema } = useTableDefinition(providerId ?? 0, 'pipeline_runs')

  const columnDefs = useMemo(() => {
    if (!tableSchema) {
      return []
    }

    return filterVisibleColumns(tableSchema.columns)
  }, [tableSchema])

  const error = queryError instanceof Error ? queryError.message : null

  const isPipelineStatus = (value: unknown): value is PipelineStatus => {
    return typeof value === 'string' &&
      ['success', 'failed', 'running', 'pending', 'cancelled', 'skipped'].includes(value)
  }

  const isValidDateValue = (value: unknown): value is string | number | Date => {
    if (typeof value === 'string' || typeof value === 'number' || value instanceof Date) {
      return true
    }
    
return false
  }

  const isRunning = runDetails?.status === 'running' || runDetails?.status === 'pending'
  const isPending = runDetails?.status === 'pending'

  const handleRerun = async () => {
    if (!runDetails) {
      return
    }

    try {
      console.log('[WorkflowLogs] Re-running workflow with inputs:', runDetails.inputs)

      const result = await rerunMutation.mutateAsync({
        pipelineId,
        inputs: runDetails.inputs,
      })

      if (onRerunSuccess && result.newRunNumber > 0) {
        onClose()
        await onRerunSuccess(pipelineId, result.newRunNumber)
      } else {
        onClose()
      }
    } catch (error: unknown) {
      console.error('[WorkflowLogs] Failed to re-run workflow:', error)
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
        await new Promise((resolve) => setTimeout(resolve, 1000))

        if (abortRef.current) {
break
}

        try {
          const { data: freshDetails } = await refetch()

          if (freshDetails && freshDetails.status === 'cancelled') {
            statusChanged = true
          }
        } catch { /* refetch may fail during polling */ }

        attempt++
      }

      if (abortRef.current) {
return
}

      if (!statusChanged) {
        await refetch()
      }

      if (onCancelSuccess) {
        await onCancelSuccess()
      }
    } catch (error: unknown) {
      if (abortRef.current) {
return
}

      const errorMsg = error instanceof Error ? error.message : 'Failed to cancel run'

      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    } finally {
      if (!abortRef.current) {
        setCancelling(false)
      }
    }
  }

  const footer = runDetails ? (
    <Group justify="flex-end" gap="xs" wrap="wrap">
      {isRunning ? (
        <Button
          variant="light"
          color="red"
          size="sm"
          style={{ flex: isMobile ? 1 : undefined }}
          leftSection={
            <IconSquare
              size={14}
              style={{
                animation: cancelling ? 'spin 1s linear infinite' : 'none',
              }}
            />
          }
          onClick={handleCancel}
          disabled={cancelling || isPending}
          title={isPending ? 'Cannot cancel pending workflow' : 'Stop the running workflow'}
        >
          {isMobile ? 'Stop' : 'Stop Workflow'}
        </Button>
      ) : (
        <Button
          variant="light"
          color="blue"
          size="sm"
          style={{ flex: isMobile ? 1 : undefined }}
          leftSection={
            <IconPlayerPlay
              size={14}
              style={{
                animation: rerunMutation.isPending ? 'spin 1s linear infinite' : 'none',
              }}
            />
          }
          onClick={handleRerun}
          disabled={rerunMutation.isPending || isPending}
          title={isPending ? 'Workflow is pending' : 'Re-run workflow with same parameters'}
        >
          {isMobile ? 'Re-run' : 'Re-run Workflow'}
        </Button>
      )}
      <Button
        component="a"
        href={runDetails.logs_url}
        target="_blank"
        rel="noopener noreferrer"
        variant="light"
        color="blue"
        size="sm"
        style={{ flex: isMobile ? 1 : undefined }}
        rightSection={<IconExternalLink size={14} />}
      >
        {isMobile ? 'Logs' : 'View Full Logs'}
      </Button>
    </Group>
  ) : null

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Run Details"
      footer={footer}
      contentPadding={false}
    >
      {loading && !runDetails ? (
        <Box py={isMobile ? 'md' : 'xl'}>
          <Group justify="center" gap="xs">
            <Loader size="sm" />
            <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>Loading run details...</Text>
          </Group>
        </Box>
      ) : error ? (
        <Alert color="red" title="Error">
          <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size}>{error}</Text>
        </Alert>
      ) : runDetails ? (
        <Stack gap={isMobile ? 'xs' : 'md'} p="md">
          {isRunning && !isMobile && (
            <Alert color="blue" icon={<IconRefresh size={16} />} title="Build in Progress">
              <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size}>
                Details will automatically update every 5 seconds.
              </Text>
            </Alert>
          )}

          <Paper
            p={isMobile ? 'md' : 'lg'}
            withBorder
            radius="md"
            style={{
              backgroundColor: 'var(--mantine-color-dark-8)',
              borderColor: 'var(--mantine-color-dark-5)',
            }}
          >
              {columnDefs.length > 0 ? (
                <Stack gap={0}>
                  {columnDefs
                    .filter(col => col.id !== 'actions')
                    .map((col, index) => {
                      const value = getValueByPath(runDetails, col.field_path)

                      if ((value === null || value === undefined) && col.id !== 'status' && col.id !== 'run_number') {
                        return null
                      }

                      return (
                        <Box
                          key={col.id}
                          py={isMobile ? 'xs' : 'sm'}
                          style={{
                            borderBottom: index === columnDefs.filter(col => col.id !== 'actions').length - 1
                              ? 'none'
                              : '1px solid var(--mantine-color-dark-6)',
                          }}
                        >
                          <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                            <Text
                              size={THEME_TYPOGRAPHY.FIELD_VALUE.size}
                              c={THEME_COLORS.FIELD_LABEL}
                              style={{
                                minWidth: isMobile ? '90px' : '130px',
                                flexShrink: 0,
                              }}
                            >
                              {col.label}
                            </Text>
                            <Box style={{ flex: 1, display: 'flex', justifyContent: 'flex-end', alignItems: 'center', overflow: 'hidden' }}>
                              {col.id === 'run_number' && value !== null && value !== undefined ? (
                                <Group gap="sm" wrap="nowrap">
                                  <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>#{String(value)}</Text>
                                  <CopyButton value={String(value)} size="sm" />
                                </Group>
                              ) : col.id === 'status' && isPipelineStatus(value) ? (
                                <StatusBadge status={value} size="md" withIcon />
                              ) : col.id === 'id' && typeof value === 'string' ? (
                                <Group gap="sm" wrap="nowrap">
                                  <Code
                                    c={THEME_COLORS.VALUE_TEXT}
                                    style={{
                                      wordBreak: 'break-all',
                                      fontSize: '0.8125rem',
                                      backgroundColor: 'var(--mantine-color-dark-7)',
                                      padding: '4px 10px',
                                      borderRadius: '6px',
                                      border: '1px solid var(--mantine-color-dark-5)',
                                    }}
                                  >
                                    {value}
                                  </Code>
                                  <CopyButton value={value} size="sm" />
                                </Group>
                              ) : col.id === 'commit_sha' && typeof value === 'string' ? (
                                <Group gap="sm" wrap="nowrap">
                                  <Code
                                    c={THEME_COLORS.VALUE_TEXT}
                                    style={{
                                      fontSize: '0.8125rem',
                                      backgroundColor: 'var(--mantine-color-dark-7)',
                                      padding: '4px 10px',
                                      borderRadius: '6px',
                                      border: '1px solid var(--mantine-color-dark-5)',
                                    }}
                                  >
                                    {value.substring(0, 7)}
                                  </Code>
                                  <CopyButton value={value} size="sm" />
                                </Group>
                              ) : col.id === 'branch' && typeof value === 'string' ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{value}</Text>
                              ) : col.id === 'duration_seconds' && typeof value === 'number' ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{formatDuration(value)}</Text>
                              ) : col.id === 'started_at' && isValidDateValue(value) ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{new Date(value).toLocaleString()}</Text>
                              ) : col.id === 'concluded_at' && isValidDateValue(value) ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{new Date(value).toLocaleString()}</Text>
                              ) : col.id === 'commit_message' && typeof value === 'string' ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT} style={{ textAlign: 'right' }} lineClamp={2}>{value}</Text>
                              ) : col.id === 'actor' && typeof value === 'string' ? (
                                <Group gap="sm" wrap="nowrap">
                                  <Avatar size="sm" radius="xl" color="blue" />
                                  <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{value}</Text>
                                </Group>
                              ) : (
                                <Box style={{ textAlign: 'right' }}>{DynamicRenderers.render(col.renderer, value as import('../../utils/dynamicRenderers').CellValue, isMobile)}</Box>
                              )}
                            </Box>
                          </Group>
                        </Box>
                      )
                    })}
                </Stack>
              ) : (
                <Stack gap={0}>
                  <Box
                    py={isMobile ? 'xs' : 'sm'}
                    style={{
                      borderBottom: '1px solid var(--mantine-color-dark-6)',
                    }}
                  >
                    <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                      <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                        Run Number
                      </Text>
                      <Group gap="sm" wrap="nowrap">
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>#{runDetails.run_number}</Text>
                        <CopyButton value={runDetails.run_number.toString()} size="sm" />
                      </Group>
                    </Group>
                  </Box>

                  <Box
                    py={isMobile ? 'xs' : 'sm'}
                    style={{
                      borderBottom: '1px solid var(--mantine-color-dark-6)',
                    }}
                  >
                    <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                      <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                        Status
                      </Text>
                      <StatusBadge status={runDetails.status} size="md" withIcon />
                    </Group>
                  </Box>

                  {runDetails.branch && (
                    <Box
                      py={isMobile ? 'xs' : 'sm'}
                      style={{
                        borderBottom: '1px solid var(--mantine-color-dark-6)',
                      }}
                    >
                      <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                          Branch
                        </Text>
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{runDetails.branch}</Text>
                      </Group>
                    </Box>
                  )}

                  <Box
                    py={isMobile ? 'xs' : 'sm'}
                    style={{
                      borderBottom: '1px solid var(--mantine-color-dark-6)',
                    }}
                  >
                    <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                      <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                        Duration
                      </Text>
                      <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{formatDuration(runDetails.duration_seconds)}</Text>
                    </Group>
                  </Box>

                  <Box
                    py={isMobile ? 'xs' : 'sm'}
                    style={{
                      borderBottom: '1px solid var(--mantine-color-dark-6)',
                    }}
                  >
                    <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                      <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                        Started At
                      </Text>
                      <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{new Date(runDetails.started_at).toLocaleString()}</Text>
                    </Group>
                  </Box>

                  {runDetails.concluded_at && (
                    <Box
                      py={isMobile ? 'xs' : 'sm'}
                      style={{
                        borderBottom: '1px solid var(--mantine-color-dark-6)',
                      }}
                    >
                      <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                          Concluded At
                        </Text>
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{new Date(runDetails.concluded_at).toLocaleString()}</Text>
                      </Group>
                    </Box>
                  )}

                  {runDetails.actor && (
                    <Box
                      py={isMobile ? 'xs' : 'sm'}
                    >
                      <Group justify="space-between" align="flex-start" wrap="nowrap" gap="xl">
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.FIELD_LABEL} style={{ minWidth: isMobile ? '90px' : '130px', flexShrink: 0 }}>
                          Actor
                        </Text>
                        <Group gap="sm" wrap="nowrap">
                          <Avatar size="sm" radius="xl" color="blue" />
                          <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{runDetails.actor}</Text>
                        </Group>
                      </Group>
                    </Box>
                  )}
                </Stack>
              )}
            </Paper>
          </Stack>
        ) : null}
      <style>{`
        @keyframes spin {
          from {
            transform: rotate(0deg);
          }
          to {
            transform: rotate(360deg);
          }
        }
      `}</style>
    </StandardModal>
  )
}
