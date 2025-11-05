import { useEffect, useState } from 'react'

import { Alert, Avatar, Box, Button, Code, Group, Loader, Paper, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconExternalLink, IconPlayerPlay, IconRefresh, IconSquare } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { useTableSchema } from '../../contexts/TableSchemaContext'
import { tauriService } from '../../services/tauri'
import type { ColumnDefinition, PipelineRun } from '../../types'
import { filterVisibleColumns } from '../../utils/columnBuilder'
import { DynamicRenderers, THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { formatDuration } from '../../utils/formatDuration'
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
  const [runDetails, setRunDetails] = useState<PipelineRun | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [rerunning, setRerunning] = useState(false)
  const [cancelling, setCancelling] = useState(false)
  const [columnDefs, setColumnDefs] = useState<ColumnDefinition[]>([])
  const isMobile = useIsMobile()
  const { getTableSchema } = useTableSchema()

  const fetchRunDetails = async () => {
    try {
      setLoading(true)
      setError(null)
      const details = await tauriService.getWorkflowRunDetails(pipelineId, runNumber)


      setRunDetails(details)
    } catch (err: any) {
      const errorMsg = err?.error || err?.message || 'Failed to fetch run details'


      setError(errorMsg)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (!opened) {
      setRunDetails(null)
      setLoading(true)
      setError(null)

return
    }

    fetchRunDetails()

    const interval = setInterval(() => {
      if (runDetails && (runDetails.status === 'running' || runDetails.status === 'pending')) {
        fetchRunDetails()
      }
    }, 5000)

    return () => clearInterval(interval)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, pipelineId, runNumber])

  useEffect(() => {
    if (!opened || !runDetails) {
return
}

    const isRunning = runDetails.status === 'running' || runDetails.status === 'pending'

    if (!isRunning) {
      const timeoutId = setTimeout(() => {
        fetchRunDetails()
      }, 1000)



return () => clearTimeout(timeoutId)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runDetails?.status, opened])

  // Load column definitions from schema
  useEffect(() => {
    if (!opened || !providerId) {
      return
    }

    const loadSchema = async () => {
      try {
        const tableSchema = await getTableSchema(providerId, 'pipeline_runs')


        if (tableSchema) {
          const visibleCols = filterVisibleColumns(tableSchema.columns)


          setColumnDefs(visibleCols)
        }
      } catch (err) {
        console.error('Failed to load table schema for modal:', err)
      }
    }

    loadSchema()
  }, [opened, providerId, getTableSchema])

  // Helper to extract value by field path
  const getValueByPath = (record: any, path: string): any => {
    const parts = path.split('.')
    let value: any = record


    for (const part of parts) {
      if (value === null || value === undefined) {
return undefined
}
      value = value[part]
    }

return value
  }

  const isRunning = runDetails?.status === 'running' || runDetails?.status === 'pending'
  const isPending = runDetails?.status === 'pending'

  const handleRerun = async () => {
    if (!runDetails) {
return
}

    setRerunning(true)

    try {
      console.log('[WorkflowLogs] Re-running workflow with inputs:', runDetails.inputs)

      const result = await tauriService.triggerPipeline({
        workflow_id: pipelineId,
        inputs: runDetails.inputs,
      })

      console.log('[WorkflowLogs] Trigger response:', result)

      let newRunNumber = 0

      try {
        const parsed = JSON.parse(result)


        newRunNumber = parsed.run_number || parsed.build_number || parsed.number || 0
      } catch {
        console.warn('[WorkflowLogs] Could not parse trigger response')
      }


      if (onRerunSuccess && newRunNumber > 0) {
        console.log('[WorkflowLogs] Polling for new run #', newRunNumber)

        let retries = 10
        let newRunReady = false

        while (retries > 0 && !newRunReady) {
          await new Promise(resolve => setTimeout(resolve, 1000))

          try {
            const newRunDetails = await tauriService.getWorkflowRunDetails(pipelineId, newRunNumber)

            if (newRunDetails && newRunDetails.status) {
              newRunReady = true
              console.log(`[WorkflowLogs] New run ready with status: ${newRunDetails.status}`)
            } else {
              console.log(`[WorkflowLogs] Run exists but no status yet, retrying... (${retries} attempts left)`)
            }
          } catch {
            console.log(`[WorkflowLogs] Run not ready yet, retrying... (${retries} attempts left)`)
          }

          retries--
        }

        onClose()
        if (onRerunSuccess) {
          await onRerunSuccess(pipelineId, newRunNumber)
        }

        setTimeout(() => setRerunning(false), 100)
      } else {
        onClose()
        setRerunning(false)
      }
    } catch (error: any) {
      console.error('[WorkflowLogs] Failed to re-run workflow:', error)
      const errorMsg = error?.error || error?.message || 'Failed to re-run workflow'


      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
      setRerunning(false)
    }
  }

  const handleCancel = async () => {
    if (!runDetails) {
return
}

    setCancelling(true)

    try {
      console.log('[WorkflowLogs] Cancelling run #', runNumber)

      await tauriService.cancelPipelineRun(pipelineId, runNumber)

      console.log('[WorkflowLogs] Cancel request sent, waiting for confirmation...')

      const maxAttempts = 10
      let attempt = 0
      let statusChanged = false

      while (attempt < maxAttempts && !statusChanged) {
        await new Promise(resolve => setTimeout(resolve, 1000))

        try {
          const freshDetails = await tauriService.getWorkflowRunDetails(pipelineId, runNumber)

          setRunDetails(freshDetails)

          if (freshDetails && (freshDetails.status === 'cancelled' as any)) {
            statusChanged = true
            console.log(`[WorkflowLogs] Cancel confirmed after ${attempt + 1} seconds - status: ${freshDetails.status}`)
          } else if (freshDetails) {
            console.log(`[WorkflowLogs] Attempt ${attempt + 1}: status still ${freshDetails.status}`)
          }
        } catch (error) {
          console.warn(`[WorkflowLogs] Failed to fetch run details on attempt ${attempt + 1}:`, error)
        }

        attempt++
      }

      if (!statusChanged) {
        console.warn('[WorkflowLogs] Cancel timeout - status not updated after 10s')
        await fetchRunDetails()
      }

      if (onCancelSuccess) {
        await onCancelSuccess()
      }
    } catch (error: any) {
      console.error('[WorkflowLogs] Failed to cancel run:', error)
      const errorMsg = error?.error || error?.message || 'Failed to cancel run'

      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    } finally {
      setCancelling(false)
    }
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Run Details"
    >
      <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
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
          <>
            {isRunning && !isMobile && (
              <Alert color="blue" icon={<IconRefresh size={16} />} title="Build in Progress" style={{ flexShrink: 0 }}>
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
                flex: 1,
                display: 'flex',
                flexDirection: 'column',
                overflow: 'auto',
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
                                  <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>#{value}</Text>
                                  <CopyButton value={value.toString()} size="sm" />
                                </Group>
                              ) : col.id === 'status' && value ? (
                                <StatusBadge status={value} size="md" withIcon={true} />
                              ) : col.id === 'id' && value ? (
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
                              ) : col.id === 'commit_sha' && value ? (
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
                              ) : col.id === 'branch' && value ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{value}</Text>
                              ) : col.id === 'duration_seconds' && value !== null && value !== undefined ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{formatDuration(value)}</Text>
                              ) : col.id === 'started_at' && value ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{new Date(value).toLocaleString()}</Text>
                              ) : col.id === 'concluded_at' && value ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{new Date(value).toLocaleString()}</Text>
                              ) : col.id === 'commit_message' && value ? (
                                <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT} style={{ textAlign: 'right' }} lineClamp={2}>{value}</Text>
                              ) : col.id === 'actor' && value ? (
                                <Group gap="sm" wrap="nowrap">
                                  <Avatar size="sm" radius="xl" color="blue" />
                                  <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{value}</Text>
                                </Group>
                              ) : (
                                <Box style={{ textAlign: 'right' }}>{DynamicRenderers.render(col.renderer, value, isMobile)}</Box>
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
                      <StatusBadge status={runDetails.status} size="md" withIcon={true} />
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

            {/* Sticky footer */}
            <Box
              style={{
                borderTop: '1px solid var(--mantine-color-default-border)',
                paddingTop: isMobile ? 8 : 12,
                marginTop: 0,
                flexShrink: 0,
              }}
            >
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
                          animation: rerunning ? 'spin 1s linear infinite' : 'none',
                        }}
                      />
                    }
                    onClick={handleRerun}
                    disabled={rerunning || isPending}
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
            </Box>
          </>
        ) : null}
      </Stack>
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
