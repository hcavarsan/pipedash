import { useEffect, useState } from 'react'

import { ActionIcon, Alert, Avatar, Badge, Box, Button, Code, Group, Loader, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconExternalLink, IconPlayerPlay, IconRefresh, IconReload, IconSquare } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { useTableSchema } from '../../contexts/TableSchemaContext'
import { tauriService } from '../../services/tauri'
import type { ColumnDefinition, PipelineRun } from '../../types'
import { filterVisibleColumns } from '../../utils/columnBuilder'
import { DynamicRenderers } from '../../utils/dynamicRenderers'
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
  const [refreshing, setRefreshing] = useState(false)
  const [columnDefs, setColumnDefs] = useState<ColumnDefinition[]>([])
  const isMobile = useIsMobile()
  const { getTableSchema } = useTableSchema()

  const fetchRunDetails = async (showRefreshLoader = false) => {
    try {
      if (showRefreshLoader) {
        setRefreshing(true)
      } else {
        setLoading(true)
      }
      setError(null)
      const details = await tauriService.getWorkflowRunDetails(pipelineId, runNumber)


      setRunDetails(details)
    } catch (err: any) {
      const errorMsg = err?.error || err?.message || 'Failed to fetch run details'


      setError(errorMsg)
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }

  useEffect(() => {
    if (!opened) {
      setRunDetails(null)
      setLoading(true)
      setError(null)

return
    }

    fetchRunDetails(false)

    const interval = setInterval(() => {
      if (runDetails && (runDetails.status === 'running' || runDetails.status === 'pending')) {
        fetchRunDetails(false)
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
        fetchRunDetails(false)
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

  const handleManualRefresh = () => {
    fetchRunDetails(true)
  }

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
        await fetchRunDetails(false)
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
      title={
        <Group gap={isMobile ? 'xs' : 'sm'} align="center" justify="space-between" style={{ width: '100%', paddingRight: '40px' }}>
          <Group gap={isMobile ? 'xs' : 'sm'} align="center">
            <Text fw={600} size={isMobile ? 'sm' : 'md'}>Run Details</Text>
            {runDetails && (
              <>
                <Badge size={isMobile ? 'sm' : 'lg'} variant="light" color="gray">
                  #{runDetails.run_number}
                </Badge>
                {isRunning && <Loader size="xs" />}
              </>
            )}
          </Group>
          {!loading && runDetails && (
            <ActionIcon
              variant="subtle"
              color="blue"
              size={isMobile ? 'sm' : 'md'}
              onClick={handleManualRefresh}
              disabled={refreshing}
              title="Refresh run details"
              style={{
                backgroundColor: 'transparent',
                cursor: refreshing ? 'not-allowed' : 'pointer',
              }}
            >
              <IconReload
                size={16}
                style={{
                  animation: refreshing ? 'spin 1s linear infinite' : 'none',
                }}
              />
            </ActionIcon>
          )}
        </Group>
      }
    >
      <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {loading && !runDetails ? (
          <Box py={isMobile ? 'md' : 'xl'}>
            <Group justify="center" gap="xs">
              <Loader size="sm" />
              <Text size="sm" c="dimmed">Loading run details...</Text>
            </Group>
          </Box>
        ) : error ? (
          <Alert color="red" title="Error">
            <Text size="sm">{error}</Text>
          </Alert>
        ) : runDetails ? (
          <>
            {isRunning && !isMobile && (
              <Alert color="blue" icon={<IconRefresh size={16} />} title="Build in Progress" style={{ flexShrink: 0 }}>
                <Text size="sm">
                  Details will automatically update every 5 seconds.
                </Text>
              </Alert>
            )}

            <Paper p={isMobile ? 'sm' : 'lg'} withBorder style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'auto' }}>
              {columnDefs.length > 0 ? (
                <SimpleGrid cols={{ base: 1, sm: 2 }} spacing={isMobile ? 'sm' : 'lg'}>
                  {columnDefs
                    .filter(col => col.id !== 'actions')
                    .map((col) => {
                      const value = getValueByPath(runDetails, col.field_path)

                      if ((value === null || value === undefined) && col.id !== 'status' && col.id !== 'run_number') {
                        return null
                      }

                      return (
                        <Box key={col.id}>
                          <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>
                            {col.label}
                          </Text>
                          {col.id === 'run_number' && value !== null && value !== undefined ? (
                            <Group gap="xs">
                              <Text fw={500}>#{value}</Text>
                              <CopyButton value={value.toString()} size="xs" />
                            </Group>
                          ) : col.id === 'status' && value ? (
                            <StatusBadge status={value} size={isMobile ? 'sm' : 'md'} withIcon={true} />
                          ) : col.id === 'id' && value ? (
                            <Group gap="xs">
                              <Code style={{ wordBreak: 'break-all', fontSize: isMobile ? '0.7rem' : undefined }}>
                                {value}
                              </Code>
                              <CopyButton value={value} size="xs" />
                            </Group>
                          ) : col.id === 'commit_sha' && value ? (
                            <Group gap="xs">
                              <Code style={{ fontSize: isMobile ? '0.75rem' : undefined }}>
                                {value.substring(0, 7)}
                              </Code>
                              <CopyButton value={value} size="xs" />
                            </Group>
                          ) : col.id === 'branch' && value ? (
                            <Badge variant="light" color="blue" size={isMobile ? 'sm' : 'md'}>
                              {value}
                            </Badge>
                          ) : col.id === 'duration_seconds' && value !== null && value !== undefined ? (
                            <Text fw={500} size={isMobile ? 'sm' : undefined}>{formatDuration(value)}</Text>
                          ) : col.id === 'started_at' && value ? (
                            <Text size={isMobile ? 'sm' : undefined}>{new Date(value).toLocaleString()}</Text>
                          ) : col.id === 'concluded_at' && value ? (
                            <Text size={isMobile ? 'sm' : undefined}>{new Date(value).toLocaleString()}</Text>
                          ) : col.id === 'commit_message' && value ? (
                            <Text size="sm">{value}</Text>
                          ) : col.id === 'actor' && value ? (
                            <Group gap="xs">
                              <Avatar size="sm" radius="xl" />
                              <Text fw={500}>{value}</Text>
                            </Group>
                          ) : (
                            <Box>{DynamicRenderers.render(col.renderer, value)}</Box>
                          )}
                        </Box>
                      )
                    })}
                </SimpleGrid>
              ) : (
                <SimpleGrid cols={{ base: 1, sm: 2 }} spacing={isMobile ? 'sm' : 'lg'}>
                  <Stack gap={isMobile ? 'xs' : 'md'}>
                    <Box>
                      <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Run Number</Text>
                      <Group gap="xs">
                        <Text fw={500}>#{runDetails.run_number}</Text>
                        <CopyButton value={runDetails.run_number.toString()} size="xs" />
                      </Group>
                    </Box>

                    <Box>
                      <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Status</Text>
                      <StatusBadge status={runDetails.status} size={isMobile ? 'sm' : 'md'} withIcon={true} />
                    </Box>

                    {runDetails.branch && (
                      <Box>
                        <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Branch</Text>
                        <Badge variant="light" color="blue" size={isMobile ? 'sm' : 'md'}>
                          {runDetails.branch}
                        </Badge>
                      </Box>
                    )}

                    <Box>
                      <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Duration</Text>
                      <Text fw={500} size={isMobile ? 'sm' : undefined}>{formatDuration(runDetails.duration_seconds)}</Text>
                    </Box>
                  </Stack>

                  <Stack gap={isMobile ? 'xs' : 'md'}>
                    <Box>
                      <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Started At</Text>
                      <Text size={isMobile ? 'sm' : undefined}>{new Date(runDetails.started_at).toLocaleString()}</Text>
                    </Box>

                    {runDetails.concluded_at && (
                      <Box>
                        <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Concluded At</Text>
                        <Text size={isMobile ? 'sm' : undefined}>{new Date(runDetails.concluded_at).toLocaleString()}</Text>
                      </Box>
                    )}

                    {runDetails.actor && (
                      <Box>
                        <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Actor</Text>
                        <Group gap="xs">
                          <Avatar size="sm" radius="xl" />
                          <Text fw={500}>{runDetails.actor}</Text>
                        </Group>
                      </Box>
                    )}
                  </Stack>
                </SimpleGrid>
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
