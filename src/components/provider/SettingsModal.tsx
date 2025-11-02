import { useEffect, useState } from 'react'

import {
  Badge,
  Box,
  Button,
  Divider,
  Group,
  NumberInput,
  Select,
  Stack,
  Switch,
  Text,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'

import { usePlugins } from '../../contexts/PluginContext'
import { useMetrics } from '../../hooks/useMetrics'
import { tauriService } from '../../services/tauri'
import type { ProviderSummary } from '../../types'
import { StandardModal } from '../common/StandardModal'

const RETENTION_OPTIONS = [
  { value: '7', label: '7 days (1 week)' },
  { value: '14', label: '14 days (2 weeks)' },
  { value: '30', label: '30 days (1 month)' },
  { value: '60', label: '60 days (2 months)' },
  { value: '90', label: '90 days (3 months)' },
  { value: 'custom', label: 'Custom...' },
]

interface SettingsModalProps {
  opened: boolean;
  onClose: () => void;
  providers: ProviderSummary[];
  onRemoveProvider: (id: number, name: string) => Promise<void>;
  onRefresh?: () => Promise<void>;
}

export const SettingsModal = ({
  opened,
  onClose,
  providers,
  onRemoveProvider,
  onRefresh,
}: SettingsModalProps) => {
  const { getPluginDisplayName } = usePlugins()
  const { configLoading: metricsLoading, getGlobalConfig, updateGlobalConfig } = useMetrics()
  const [editingId, setEditingId] = useState<number | null>(null)
  const [refreshValues, setRefreshValues] = useState<Record<number, number>>({})
  const [saving, setSaving] = useState(false)
  const [metricsEnabled, setMetricsEnabled] = useState(false)
  const [metricsRetention, setMetricsRetention] = useState(7)
  const [metricsRetentionMode, setMetricsRetentionMode] = useState<'preset' | 'custom'>('preset')
  const [editingMetrics, setEditingMetrics] = useState(false)
  const [cacheStats, setCacheStats] = useState<{
    pipelines_count: number
    run_history_count: number
    workflow_params_count: number
    metrics_count: number
  } | null>(null)
  const [loadingCache, setLoadingCache] = useState(false)

  useEffect(() => {
    if (opened) {
      loadMetricsConfig()
      loadCacheStats()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened])

  const loadMetricsConfig = async () => {
    const config = await getGlobalConfig()

    if (config) {
      setMetricsEnabled(config.enabled)
      const retention = config.default_retention_days


      setMetricsRetention(retention)
      if ([7, 14, 30, 60, 90].includes(retention)) {
        setMetricsRetentionMode('preset')
      } else {
        setMetricsRetentionMode('custom')
      }
    }
  }

  const loadCacheStats = async () => {
    try {
      const stats = await tauriService.getCacheStats()

      setCacheStats(stats)
    } catch (error) {
      console.error('Failed to load cache stats:', error)
    }
  }

  const handleSaveMetrics = async () => {
    const success = await updateGlobalConfig(metricsEnabled, metricsRetention)

    if (success) {
      notifications.show({
        title: 'Metrics Configuration Updated',
        message: 'Global metrics settings saved successfully',
        color: 'green',
      })
      setEditingMetrics(false)
    }
  }

  const handleClearCache = async (type: 'pipelines' | 'run_history' | 'workflow_params' | 'all') => {
    const titles = {
      pipelines: 'Clear Pipelines Cache',
      run_history: 'Clear Run History Cache',
      workflow_params: 'Clear Workflow Parameters Cache',
      all: 'Clear All Caches',
    }

    const messages = {
      pipelines: 'This will clear all cached pipeline data. It will be re-fetched on next refresh.',
      run_history: 'This will clear all cached run history data. It will be re-fetched when needed.',
      workflow_params: 'This will clear cached workflow parameters. They will be re-fetched when needed.',
      all: 'This will clear all caches (pipelines, run history, and workflow parameters). All data will be re-fetched as needed.',
    }

    modals.openConfirmModal({
      title: titles[type],
      children: <Text size="sm">{messages[type]}</Text>,
      labels: { confirm: 'Clear', cancel: 'Cancel' },
      confirmProps: { color: 'blue' },
      onConfirm: async () => {
        setLoadingCache(true)
        try {
          if (type === 'pipelines') {
            await tauriService.clearPipelinesCache()
          } else if (type === 'run_history') {
            await tauriService.clearAllRunHistoryCaches()
          } else if (type === 'workflow_params') {
            await tauriService.clearWorkflowParamsCache()
          } else {
            await tauriService.clearAllCaches()
          }

          if (onRefresh) {
            await onRefresh()
          }

          await new Promise(resolve => setTimeout(resolve, 100))

          await loadCacheStats()

          notifications.show({
            title: 'Success',
            message: `${titles[type]} completed`,
            color: 'green',
          })
        } catch (error: any) {
          notifications.show({
            title: 'Error',
            message: error?.error || error?.message || 'Failed to clear cache',
            color: 'red',
          })
        } finally {
          setLoadingCache(false)
        }
      },
    })
  }

  const handleRemove = (id: number, name: string) => {
    modals.openConfirmModal({
      title: 'Remove Provider',
      children: (
        <Text size="sm">
          Are you sure you want to remove <strong>{name}</strong>? All cached pipeline data will be deleted.
        </Text>
      ),
      labels: { confirm: 'Remove', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        try {
          await onRemoveProvider(id, name)
        } catch (error) {
          console.error('Failed to remove provider:', error)
        }
      },
    })
  }

  const handleEditRefreshInterval = (provider: ProviderSummary) => {
    setEditingId(provider.id)
    setRefreshValues({
      ...refreshValues,
      [provider.id]: provider.refresh_interval,
    })
  }

  const handleSaveRefreshInterval = async (provider: ProviderSummary) => {
    const newValue = refreshValues[provider.id] ?? provider.refresh_interval

    if (newValue < 5 || newValue > 300) {
      notifications.show({
        title: 'Invalid Value',
        message: 'Refresh interval must be between 5 and 300 seconds',
        color: 'red',
      })

return
    }

    setSaving(true)
    try {
      await tauriService.updateProviderRefreshInterval(provider.id, newValue)

      if (onRefresh) {
        await onRefresh()
      }

      notifications.show({
        title: 'Updated',
        message: `Refresh interval updated for ${provider.name}`,
        color: 'green',
      })

      setEditingId(null)
    } catch (error: any) {
      notifications.show({
        title: 'Error',
        message: error?.error || error?.message || 'Failed to update',
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const handleCancelEdit = () => {
    setEditingId(null)
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Settings"
    >
      <Stack gap="lg">
        <Box>
          <Text size="sm" fw={600} c="dimmed" mb="sm" tt="uppercase">
            Providers
          </Text>

          {providers.length === 0 ? (
            <Text size="sm" c="dimmed" ta="center" py="lg">
              No providers configured
            </Text>
          ) : (
            <Stack gap="sm">
              {providers.map((provider) => {
                const isEditing = editingId === provider.id
                const currentValue = refreshValues[provider.id] ?? provider.refresh_interval

                return (
                  <Box
                    key={provider.id}
                    p="md"
                    style={{
                      border: '1px solid var(--mantine-color-dark-5)',
                      borderRadius: '8px',
                      backgroundColor: 'var(--mantine-color-dark-8)',
                    }}
                  >
                    <Stack gap="sm">
                      <Group justify="space-between" align="flex-start" wrap="wrap">
                        <Box style={{ flex: 1 }}>
                          <Text fw={600} size="md" mb={4}>
                            {provider.name}
                          </Text>
                          <Text size="sm" c="dimmed">
                            {getPluginDisplayName(provider.provider_type)} · {provider.pipeline_count} pipeline{provider.pipeline_count !== 1 ? 's' : ''}
                          </Text>
                        </Box>

                        {!isEditing && (
                          <Button
                            size="xs"
                            color="red"
                            variant="subtle"
                            onClick={() => handleRemove(provider.id, provider.name)}
                          >
                            Remove
                          </Button>
                        )}
                      </Group>

                      <Divider />

                      <Group align="flex-end" gap="md" wrap="wrap">
                        <NumberInput
                          label="Refresh Interval"
                          description="Seconds between data fetches (5-300)"
                          value={currentValue}
                          onChange={(val) =>
                            setRefreshValues({
                              ...refreshValues,
                              [provider.id]: Number(val) || 30,
                            })
                          }
                          min={5}
                          max={300}
                          step={5}
                          disabled={!isEditing || saving}
                          style={{ flex: 1, maxWidth: 200 }}
                        />

                        {isEditing ? (
                          <Group gap="xs">
                            <Button
                              size="xs"
                              variant="subtle"
                              color="gray"
                              onClick={handleCancelEdit}
                              disabled={saving}
                            >
                              Cancel
                            </Button>
                            <Button
                              size="xs"
                              onClick={() => handleSaveRefreshInterval(provider)}
                              loading={saving}
                            >
                              Save
                            </Button>
                          </Group>
                        ) : (
                          <Button
                            size="xs"
                            variant="light"
                            onClick={() => handleEditRefreshInterval(provider)}
                          >
                            Edit
                          </Button>
                        )}
                      </Group>
                    </Stack>
                  </Box>
                )
              })}
            </Stack>
          )}
        </Box>

        <Divider />

        <Box>
          <Text size="sm" fw={600} c="dimmed" mb="sm" tt="uppercase">
            Metrics
          </Text>

          <Box
            p="md"
            style={{
              border: '1px solid var(--mantine-color-dark-5)',
              borderRadius: '8px',
              backgroundColor: 'var(--mantine-color-dark-8)',
            }}
          >
            <Stack gap="sm">
              <Group justify="space-between" align="flex-start">
                <Box style={{ flex: 1 }}>
                  <Text fw={600} size="md" mb={4}>
                    Global Metrics Configuration
                  </Text>
                  <Text size="sm" c="dimmed">
                    Default settings for new pipelines
                  </Text>
                </Box>
              </Group>

              <Divider />

              <Switch
                label="Enable metrics by default"
                description="New pipelines will automatically collect metrics"
                checked={metricsEnabled}
                onChange={(e) => setMetricsEnabled(e.currentTarget.checked)}
                disabled={!editingMetrics || metricsLoading}
              />

              <Stack gap="sm">
                <Select
                  label="Default Retention Period"
                  description="How long to keep metrics data"
                  value={metricsRetentionMode === 'preset' ? metricsRetention.toString() : 'custom'}
                  onChange={(val) => {
                    if (val === 'custom') {
                      setMetricsRetentionMode('custom')
                    } else {
                      setMetricsRetentionMode('preset')
                      setMetricsRetention(Number(val) || 7)
                    }
                  }}
                  data={RETENTION_OPTIONS}
                  disabled={!editingMetrics || metricsLoading}
                  style={{ maxWidth: 300 }}
                />

                {metricsRetentionMode === 'custom' && (
                  <NumberInput
                    label="Custom Retention (days)"
                    description="Enter number of days (1-90 max)"
                    value={metricsRetention}
                    onChange={(val) => setMetricsRetention(Number(val) || 7)}
                    min={1}
                    max={90}
                    disabled={!editingMetrics || metricsLoading}
                    style={{ maxWidth: 300 }}
                  />
                )}
              </Stack>

              <Group justify="flex-end" gap="xs">
                {editingMetrics ? (
                  <>
                    <Button
                      size="xs"
                      variant="subtle"
                      color="gray"
                      onClick={() => {
                        setEditingMetrics(false)
                        loadMetricsConfig()
                      }}
                      disabled={metricsLoading}
                    >
                      Cancel
                    </Button>
                    <Button
                      size="xs"
                      onClick={handleSaveMetrics}
                      loading={metricsLoading}
                    >
                      Save
                    </Button>
                  </>
                ) : (
                  <Button
                    size="xs"
                    variant="light"
                    onClick={() => setEditingMetrics(true)}
                  >
                    Edit
                  </Button>
                )}
              </Group>
            </Stack>
          </Box>
        </Box>

        <Divider />

        <Box>
          <Text size="sm" fw={600} c="dimmed" mb="sm" tt="uppercase">
            Cache Management
          </Text>

          <Box
            p="md"
            style={{
              border: '1px solid var(--mantine-color-dark-5)',
              borderRadius: '8px',
              backgroundColor: 'var(--mantine-color-dark-8)',
            }}
          >
            <Stack gap="sm">
              <Box>
                <Text fw={600} size="md" mb={4}>
                  Cached Data
                </Text>
                <Text size="sm" c="dimmed">
                  Clear cached data to force fresh data fetch
                </Text>
              </Box>

              <Divider />

              <Stack gap="sm">
                <Group justify="space-between">
                  <Box>
                    <Text size="sm" fw={500}>Pipelines Cache</Text>
                    <Text size="xs" c="dimmed">
                      {cacheStats ? `${cacheStats.pipelines_count} cached pipelines` : 'Loading...'}
                    </Text>
                  </Box>
                  <Button
                    size="xs"
                    variant="light"
                    color="blue"
                    onClick={() => handleClearCache('pipelines')}
                    loading={loadingCache}
                  >
                    Clear
                  </Button>
                </Group>

                <Group justify="space-between">
                  <Box>
                    <Text size="sm" fw={500}>Run History Cache</Text>
                    <Text size="xs" c="dimmed">
                      {cacheStats ? `${cacheStats.run_history_count} cached runs` : 'Loading...'}
                    </Text>
                  </Box>
                  <Button
                    size="xs"
                    variant="light"
                    color="blue"
                    onClick={() => handleClearCache('run_history')}
                    loading={loadingCache}
                  >
                    Clear
                  </Button>
                </Group>

                <Group justify="space-between">
                  <Box>
                    <Text size="sm" fw={500}>Workflow Parameters Cache</Text>
                    <Text size="xs" c="dimmed">
                      {cacheStats ? `${cacheStats.workflow_params_count} cached parameters` : 'Loading...'}
                    </Text>
                  </Box>
                  <Button
                    size="xs"
                    variant="light"
                    color="blue"
                    onClick={() => handleClearCache('workflow_params')}
                    loading={loadingCache}
                  >
                    Clear
                  </Button>
                </Group>

                {cacheStats && cacheStats.metrics_count > 0 && (
                  <Group justify="space-between">
                    <Box>
                      <Text size="sm" fw={500}>Metrics Data</Text>
                      <Text size="xs" c="dimmed">
                        {cacheStats.metrics_count} stored metrics
                      </Text>
                    </Box>
                    <Badge size="sm" color="blue" variant="light">
                      Managed in Metrics section
                    </Badge>
                  </Group>
                )}
              </Stack>

              <Divider />

              <Button
                fullWidth
                size="sm"
                variant="light"
                color="blue"
                onClick={() => handleClearCache('all')}
                loading={loadingCache}
              >
                Clear All Caches
              </Button>
            </Stack>
          </Box>
        </Box>

        <Divider />

        <Box>
          <Text size="sm" fw={600} c="dimmed" mb="sm" tt="uppercase">
            Application
          </Text>
          <Group justify="space-between">
            <Text size="sm" c="dimmed">Version</Text>
            <Text size="sm" fw={500}>0.1.0</Text>
          </Group>
        </Box>
      </Stack>
    </StandardModal>
  )
}
