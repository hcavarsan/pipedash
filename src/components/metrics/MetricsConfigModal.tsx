import { useEffect, useState } from 'react'

import { Button, Card, Group, NumberInput, Progress, Select, Stack, Switch, Text } from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import { IconCheck, IconTrash } from '@tabler/icons-react'

import { useMetrics } from '../../hooks/useMetrics'
import type { MetricsStats } from '../../types'
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
  const { configLoading, flushLoading, getGlobalConfig, updateGlobalConfig, getPipelineConfig, updatePipelineConfig, flushMetrics, getStorageStats } =
    useMetrics()
  const [enabled, setEnabled] = useState(false)
  const [initialEnabled, setInitialEnabled] = useState(false)
  const [retentionDays, setRetentionDays] = useState(7)
  const [retentionMode, setRetentionMode] = useState<'preset' | 'custom'>('preset')
  const [stats, setStats] = useState<MetricsStats | null>(null)
  const [loadingInitial, setLoadingInitial] = useState(true)

  useEffect(() => {
    if (opened) {
      setLoadingInitial(true)
      Promise.all([loadConfig(), loadStats()]).finally(() => setLoadingInitial(false))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, pipelineId])

  const loadConfig = async () => {
    if (pipelineId) {
      const config = await getPipelineConfig(pipelineId)

      if (config) {
        setEnabled(config.enabled)
        setInitialEnabled(config.enabled)

        const retention = config.retention_days


        setRetentionDays(retention)

        // Determine if it's a preset value or custom
        if ([7, 14, 30, 60, 90].includes(retention)) {
          setRetentionMode('preset')
        } else {
          setRetentionMode('custom')
        }
      }
    } else {
      const config = await getGlobalConfig()

      if (config) {
        setEnabled(config.enabled)
        setInitialEnabled(config.enabled)

        const retention = config.default_retention_days


        setRetentionDays(retention)

        // Determine if it's a preset value or custom
        if ([7, 14, 30, 60, 90].includes(retention)) {
          setRetentionMode('preset')
        } else {
          setRetentionMode('custom')
        }
      }
    }
  }

  const loadStats = async () => {
    const data = await getStorageStats()


    setStats(data)
  }

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
        await flushMetrics(pipelineId)

        await loadStats()
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
        await flushMetrics(pipelineId)

        await loadStats()

      }

      const success = pipelineId
        ? await updatePipelineConfig(pipelineId, enabled, retentionDays)
        : await updateGlobalConfig(enabled, retentionDays)

      if (success) {
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
      }
    } catch (error) {
      console.error('Failed to save metrics config:', error)
    }
  }

  const pipelineStats = pipelineId && stats
    ? stats.by_pipeline.find((p) => p.pipeline_id === pipelineId)
    : null
  const metricsCount = pipelineStats ? pipelineStats.metrics_count : stats?.total_metrics_count || 0
  const sizeMB = stats?.estimated_size_mb || 0

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={pipelineId ? `Configure Metrics: ${pipelineName}` : 'Global Metrics Configuration'}
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
              disabled={!enabled}
            />

            {retentionMode === 'custom' && (
              <NumberInput
                label="Custom Retention (days)"
                description="Enter number of days (1-90 max)"
                value={retentionDays}
                onChange={(val) => setRetentionDays(Number(val) || 7)}
                min={1}
                max={90}
                disabled={!enabled}
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

        {stats && (
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
                  loading={flushLoading}
                  disabled={loadingInitial}
                >
                  Flush
                </Button>
              </Group>

              <Progress value={Math.min((sizeMB / 100) * 100, 100)} size="sm" />

              {stats.last_cleanup_at && (
                <Text size="xs" c="dimmed">
                  Last cleanup: {new Date(stats.last_cleanup_at).toLocaleString()}
                </Text>
              )}
            </Stack>
          </Card>
        )}

        <Group justify="flex-end" mt="md">
          <Button
            variant="subtle"
            onClick={onClose}
            disabled={configLoading || flushLoading}
          >
            Cancel
          </Button>
          <Button
            onClick={handleSave}
            loading={configLoading}
            disabled={loadingInitial}
          >
            Save
          </Button>
        </Group>
      </Stack>
    </StandardModal>
  )
}
