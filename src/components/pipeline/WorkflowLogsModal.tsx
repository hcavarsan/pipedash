import { useEffect, useState } from 'react'

import { ActionIcon, Alert, Avatar, Badge, Box, Button, Code, Divider, Group, Loader, Paper, SimpleGrid, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconExternalLink, IconPlayerPlay, IconRefresh, IconReload, IconSquare } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { tauriService } from '../../services/tauri'
import type { PipelineRun } from '../../types'
import { formatDuration } from '../../utils/formatDuration'
import { CopyButton } from '../atoms/CopyButton'
import { StandardModal } from '../common/StandardModal'
import { StatusBadge } from '../common/StatusBadge'

interface WorkflowLogsModalProps {
  opened: boolean;
  onClose: () => void;
  pipelineId: string;
  runNumber: number;
  onRerunSuccess?: (pipelineId: string, newRunNumber: number) => void;
  onCancelSuccess?: () => void;
}

/* eslint-disable complexity */
export const WorkflowLogsModal = ({
  opened,
  onClose,
  pipelineId,
  runNumber,
  onRerunSuccess,
  onCancelSuccess,
}: WorkflowLogsModalProps) => {
  const [runDetails, setRunDetails] = useState<PipelineRun | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [rerunning, setRerunning] = useState(false)
  const [cancelling, setCancelling] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const isMobile = useIsMobile()

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

  const handleManualRefresh = () => {
    fetchRunDetails(true)
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

                  {runDetails.commit_sha && (
                    <Box>
                      <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Commit SHA</Text>
                      <Group gap="xs">
                        <Code style={{ fontSize: isMobile ? '0.75rem' : undefined }}>
                          {runDetails.commit_sha.substring(0, 7)}
                        </Code>
                        <CopyButton value={runDetails.commit_sha} size="xs" />
                      </Group>
                    </Box>
                  )}

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
                    <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Run ID</Text>
                    <Group gap="xs">
                      <Code style={{ wordBreak: 'break-all', fontSize: isMobile ? '0.7rem' : undefined }}>
                        {runDetails.id}
                      </Code>
                      <CopyButton value={runDetails.id} size="xs" />
                    </Group>
                  </Box>

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

              {runDetails.commit_message && (
                <>
                  <Divider my={isMobile ? 'xs' : 'md'} />
                  <Box>
                    <Text size="xs" c="dimmed" tt="uppercase" mb={isMobile ? 2 : 4}>Commit Message</Text>
                    <Text size="sm">{runDetails.commit_message}</Text>
                  </Box>
                </>
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
