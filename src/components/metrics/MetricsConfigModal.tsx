import { Button, Card, Group, NumberInput, Select, Skeleton, Stack, Switch, Text } from '@mantine/core'
import { modals } from '@mantine/modals'
import { IconTrash } from '@tabler/icons-react'
import { useEffect, useState } from 'react'

import { logger } from '../../lib/logger'
import {
  useFlushMetrics,
  useGlobalMetricsConfig,
  useMetricsStorageStats,
  usePipelineMetricsConfig,
  useUpdateGlobalMetricsConfig,
  useUpdatePipelineMetricsConfig,
} from '../../queries/useMetricsQueries'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'

const RETENTION_PRESETS = [7, 14, 30, 60, 90]

const RETENTION_OPTIONS = [
  { value: '7', label: '7 days (1 week)' },
  { value: '14', label: '14 days (2 weeks)' },
  { value: '30', label: '30 days (1 month)' },
  { value: '60', label: '60 days (2 months)' },
  { value: '90', label: '90 days (3 months)' },
  { value: 'custom', label: 'Custom' },
]

interface MetricsConfigModalProps {
  opened: boolean
  onClose: () => void
  pipelineId?: string
  pipelineName?: string
  onConfigChange?: () => void
}

const formatCount = (count: number) => `${count.toLocaleString()} metric${count === 1 ? '' : 's'}`

const StatRow = ({ label, value }: { label: string; value: string }) => (
  <Group justify="space-between" gap="sm" wrap="nowrap">
    <Text size="sm" c="dimmed">
      {label}
    </Text>
    <Text size="sm">{value}</Text>
  </Group>
)

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

  const configQuery = pipelineId ? pipelineConfig : globalConfig
  const config = configQuery.data

  useEffect(() => {
    if (config) {
      const retention = 'retention_days' in config ? config.retention_days : config.default_retention_days

      setEnabled(config.enabled)
      setInitialEnabled(config.enabled)
      setRetentionDays(retention)
      setRetentionMode(RETENTION_PRESETS.includes(retention) ? 'preset' : 'custom')
    }
  }, [config])

  const handleFlush = () => {
    modals.openConfirmModal({
      title: 'Delete metrics',
      children: (
        <Text size="sm">
          This permanently deletes {pipelineId ? 'the stored metrics for this pipeline' : 'all stored metrics'}. It
          cannot be undone.
        </Text>
      ),
      labels: { confirm: 'Delete metrics', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: () => flushMutation.mutate(pipelineId, { onSuccess: () => onConfigChange?.() }),
    })
  }

  const performSave = async (shouldFlush: boolean) => {
    try {
      if (shouldFlush) {
        await flushMutation.mutateAsync(pipelineId)
      }

      if (pipelineId) {
        await updatePipeline.mutateAsync({ pipelineId, enabled, retentionDays })
      } else {
        await updateGlobal.mutateAsync({ enabled, retentionDays })
      }

      onConfigChange?.()
      onClose()
    } catch (error) {
      logger.error('MetricsConfigModal', 'Failed to save metrics configuration', error)
    }
  }

  const handleSave = () => {
    if (!(initialEnabled && !enabled)) {
      void performSave(false)

      return
    }

    modals.openConfirmModal({
      title: 'Disable metrics',
      children: (
        <Text size="sm">
          Disabling metrics permanently deletes all stored metrics for {pipelineId ? 'this pipeline' : 'all pipelines'}.
          It cannot be undone.
        </Text>
      ),
      labels: { confirm: 'Disable and delete data', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: () => performSave(true),
    })
  }

  const pipelineStats = pipelineId ? stats.data?.by_pipeline.find((p) => p.pipeline_id === pipelineId) : undefined
  const isSaving = updateGlobal.isPending || updatePipeline.isPending
  const isBusy = isSaving || flushMutation.isPending

  const footer = (
    <Group justify="flex-end" gap="sm">
      <Button variant="default" size="sm" onClick={onClose} disabled={isBusy}>
        Cancel
      </Button>
      <Button size="sm" onClick={handleSave} loading={isSaving} disabled={!config || flushMutation.isPending}>
        Save
      </Button>
    </Group>
  )

  const renderStorage = () => {
    if (stats.isError) {
      return (
        <ErrorState
          title="Couldn't load storage usage"
          error={stats.error}
          onRetry={() => stats.refetch()}
          retrying={stats.isFetching}
        />
      )
    }

    if (!stats.data) {
      return <Skeleton height={64} />
    }

    return (
      <Stack gap="xs">
        {pipelineId && <StatRow label="This pipeline" value={formatCount(pipelineStats?.metrics_count ?? 0)} />}
        <StatRow
          label="All pipelines"
          value={`${formatCount(stats.data.total_metrics_count)} · ${stats.data.estimated_size_mb.toFixed(2)} MB`}
        />
        <StatRow
          label="Last cleanup (all pipelines)"
          value={stats.data.last_cleanup_at ? new Date(stats.data.last_cleanup_at).toLocaleString() : '—'}
        />
      </Stack>
    )
  }

  const renderContent = () => {
    if (configQuery.isError) {
      return (
        <ErrorState
          title="Couldn't load metrics settings"
          error={configQuery.error}
          hint="Your saved settings were not loaded, so saving is disabled."
          onRetry={() => configQuery.refetch()}
          retrying={configQuery.isFetching}
        />
      )
    }

    if (!config) {
      return <LoadingState message="Loading metrics settings..." />
    }

    return (
      <>
        <Card p="sm">
          <Stack gap="sm">
            <Text size="sm" fw={600}>
              Collection settings
            </Text>
            <Switch
              label="Enable metrics"
              description={
                pipelineId
                  ? 'Collect and store metrics for this pipeline'
                  : 'Enable metrics collection by default for new pipelines'
              }
              checked={enabled}
              onChange={(e) => setEnabled(e.currentTarget.checked)}
              disabled={isBusy}
            />

            <Select
              label="Retention period"
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
              allowDeselect={false}
              disabled={!enabled || isBusy}
            />

            {retentionMode === 'custom' && (
              <NumberInput
                label="Custom retention (days)"
                description="Enter 1 to 90 days"
                value={retentionDays}
                onChange={(val) => setRetentionDays(Number(val) || 7)}
                min={1}
                max={90}
                disabled={!enabled || isBusy}
              />
            )}
          </Stack>
        </Card>

        <Card p="sm">
          <Stack gap="sm">
            <Group justify="space-between">
              <Text size="sm" fw={600}>
                Stored metrics
              </Text>
              <Button
                size="xs"
                variant="subtle"
                color="red"
                leftSection={<IconTrash size={14} />}
                onClick={handleFlush}
                loading={flushMutation.isPending}
                disabled={!stats.data || isSaving}
              >
                {pipelineId ? 'Delete pipeline metrics' : 'Delete all metrics'}
              </Button>
            </Group>
            {renderStorage()}
          </Stack>
        </Card>
      </>
    )
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      size="md"
      title={pipelineId ? `Configure metrics: ${pipelineName}` : 'Global metrics configuration'}
      footer={footer}
    >
      <Stack gap="sm">{renderContent()}</Stack>
    </StandardModal>
  )
}
