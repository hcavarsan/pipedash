import { useEffect, useState } from 'react'

import { Button, Card, Group, NumberInput, Progress, Select, Stack, Switch, Text } from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import { IconCheck, IconTrash } from '@tabler/icons-react'

import {
  useFlushMetrics,
  useGlobalMetricsConfig,
  useMetricsStorageStats,
  usePipelineMetricsConfig,
  useUpdateGlobalMetricsConfig,
  useUpdatePipelineMetricsConfig,
} from '../../queries/useMetricsQueries'
import { StandardModal } from '../common/StandardModal'

const RETENTION_OPTIONS = [
  { value: '7', label: '7 days (1 week)' },
  { value: '14', label: '14 days (2 weeks)' },
  { value: '30', label: '30 days (1 month)' },
  { value: '60', label: '60 days (2 months)' },
  { value: '90', label: '90 days (3 months)' },
  { value: 'custom', label: 'Custom...' },
]

interface MetricsConfigModalProps {
  opened: boolean
  onClose: () => void
  pipelineId?: string
  pipelineName?: string
  onConfigChange?: () => void
}

export const MetricsConfigModal = ({
  opened,
  onClose,
  pipelineId,
  pipelineName,
  onConfigChange,
}: MetricsConfigModalProps) => {
  const globalConfig = useGlobalMetricsConfig()
  const pipelineConfig = usePipelineMetricsConfig(pipelineId || '')
  const stats = useMetricsStorageStats()
  const updateGlobal = useUpdateGlobalMetricsConfig()
  const updatePipeline = useUpdatePipelineMetricsConfig()
  const flushMutation = useFlushMetrics()

  const [enabled, setEnabled] = useState(false)
  const [initialEnabled, setInitialEnabled] = useState(false)
  const [retentionDays, setRetentionDays] = useState(7)
  const [retentionMode, setRetentionMode] = useState<'preset' | 'custom'>('preset')

  const config = pipelineId ? pipelineConfig.data : globalConfig.data
  const configLoading = pipelineId ? pipelineConfig.isLoading : globalConfig.isLoading

  useEffect(() => {
    if (config) {
      const retention = 'retention_days' in config ? config.retention_days : config.default_retention_days

      setEnabled(config.enabled)
      setInitialEnabled(config.enabled)
      setRetentionDays(retention)

      if ([7, 14, 30, 60, 90].includes(retention)) {
        setRetentionMode('preset')
      } else {
        setRetentionMode('custom')
      }
    }
  }, [config])

  const handleFlush = () => {
    modals.openConfirmModal({
      title: 'Flush Metrics',
      children: (
        <Text size="sm">
          Are you sure you want to delete {pipelineId ? 'metrics for this pipeline' : 'all metrics'}?
          This action cannot be undone.
        </Text>
      ),
      labels: { confirm: 'Delete', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        await flushMutation.mutateAsync(pipelineId)
        stats.refetch()
        onConfigChange?.()
      },
    })
  }

  const handleSave = async () => {
    const isDisabling = initialEnabled && !enabled

    if (isDisabling) {
      modals.openConfirmModal({
        title: 'Disable Metrics',
        children: (
          <Stack gap="sm">
            <Text size="sm">
              Disabling metrics will permanently delete all stored metrics data for{' '}
              {pipelineId ? 'this pipeline' : 'all pipelines'}.
            </Text>
            <Text size="sm" c="dimmed">
              This action cannot be undone.
            </Text>
          </Stack>
        ),
        labels: { confirm: 'Disable & Delete', cancel: 'Cancel' },
        confirmProps: { color: 'red', leftSection: <IconTrash size={16} /> },
        onConfirm: async () => {
          await performSave(true)
        },
      })
    } else {
      await performSave(false)
    }
  }

  const performSave = async (shouldFlush: boolean) => {
    try {
      if (shouldFlush) {
        await flushMutation.mutateAsync(pipelineId)
        stats.refetch()
      }

      if (pipelineId) {
        await updatePipeline.mutateAsync({
          pipelineId,
          enabled,
          retentionDays,
        })
      } else {
        await updateGlobal.mutateAsync({ enabled, retentionDays })
      }

      notifications.show({
        title: 'Configuration Saved',
        message: enabled
          ? initialEnabled
            ? 'Metrics updated successfully'
            : 'Metrics enabled! Collecting data from your pipeline runs...'
          : 'Metrics disabled successfully',
        color: 'green',
        icon: <IconCheck size={18} />,
      })

      onConfigChange?.()
      onClose()
    } catch (error) {
      console.error('Failed to save metrics config:', error)
    }
  }

  const pipelineStats = pipelineId && stats.data
    ? stats.data.by_pipeline.find((p) => p.pipeline_id === pipelineId)
    : null
  const metricsCount = pipelineStats ? pipelineStats.metrics_count : stats.data?.total_metrics_count || 0
  const sizeMB = stats.data?.estimated_size_mb || 0

  const loadingInitial = configLoading || stats.isLoading
  const isSaving = updateGlobal.isPending || updatePipeline.isPending

  const footer = (
    <Group justify="flex-end" gap="xs">
      <Button
        variant="subtle"
        onClick={onClose}
        disabled={isSaving || flushMutation.isPending}
      >
        Cancel
      </Button>
      <Button
        onClick={handleSave}
        loading={isSaving}
        disabled={loadingInitial}
      >
        Save
      </Button>
    </Group>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={pipelineId ? `Configure Metrics: ${pipelineName}` : 'Global Metrics Configuration'}
      footer={footer}
    >
      <Stack gap="sm">
        <Card p="sm" withBorder>
          <Stack gap="sm">
            <Text size="sm" fw={600}>
              Collection Settings
            </Text>
            <Switch
              label="Enable Metrics"
              description={
                pipelineId
                  ? 'Collect and store metrics for this pipeline'
                  : 'Enable metrics collection by default for new pipelines'
              }
              checked={enabled}
              onChange={(e) => setEnabled(e.currentTarget.checked)}
              disabled={isSaving || loadingInitial}
            />

            <Select
              label="Retention Period"
              description="How long to keep metrics data"
              value={retentionMode === 'preset' ? retentionDays.toString() : 'custom'}
              onChange={(val) => {
                if (val === 'custom') {
                  setRetentionMode('custom')
                } else {
                  setRetentionMode('preset')
                  setRetentionDays(Number(val) || 7)
                }
              }}
              data={RETENTION_OPTIONS}
              disabled={!enabled || isSaving || loadingInitial}
            />

            {retentionMode === 'custom' && (
              <NumberInput
                label="Custom Retention (days)"
                description="Enter number of days (1-90 max)"
                value={retentionDays}
                onChange={(val) => setRetentionDays(Number(val) || 7)}
                min={1}
                max={90}
                disabled={!enabled || isSaving || loadingInitial}
              />
            )}

            {enabled && (
              <Text size="xs" c="dimmed">
                Estimated storage: ~{Math.round(retentionDays * 0.1)}MB per pipeline for {retentionDays}{' '}
                days
              </Text>
            )}
          </Stack>
        </Card>

        {stats.data && (
          <Card p="sm" withBorder>
            <Stack gap="xs">
              <Group justify="space-between">
                <div>
                  <Text size="sm" fw={600}>
                    Storage Usage
                  </Text>
                  <Text size="xs" c="dimmed">
                    {metricsCount.toLocaleString()} metric{metricsCount !== 1 ? 's' : ''} • {sizeMB.toFixed(2)} MB
                  </Text>
                </div>
                <Button
                  size="xs"
                  variant="subtle"
                  color="red"
                  leftSection={<IconTrash size={14} />}
                  onClick={handleFlush}
                  loading={flushMutation.isPending}
                  disabled={loadingInitial}
                >
                  Flush
                </Button>
              </Group>

              <Progress value={Math.min((sizeMB / 100) * 100, 100)} size="sm" />

              {stats.data.last_cleanup_at && (
                <Text size="xs" c="dimmed">
                  Last cleanup: {new Date(stats.data.last_cleanup_at).toLocaleString()}
                </Text>
              )}
            </Stack>
          </Card>
        )}
      </Stack>
    </StandardModal>
  )
}
