import { useEffect, useState } from 'react'

import {
  Box,
  Card,
  Divider,
  NumberInput,
  Select,
  Stack,
  Switch,
  Text,
} from '@mantine/core'

import { useGlobalMetricsConfig, useUpdateGlobalMetricsConfig } from '../../../queries/useMetricsQueries'

const RETENTION_OPTIONS = [
  { value: '7', label: '7 days' },
  { value: '14', label: '14 days' },
  { value: '30', label: '30 days' },
  { value: '60', label: '60 days' },
  { value: '90', label: '90 days' },
  { value: 'custom', label: 'Custom' },
]

export const MetricsSection = () => {
  const globalConfig = useGlobalMetricsConfig()
  const updateGlobalMutation = useUpdateGlobalMetricsConfig()

  const [metricsEnabled, setMetricsEnabled] = useState(false)
  const [metricsRetention, setMetricsRetention] = useState(7)
  const [metricsRetentionMode, setMetricsRetentionMode] = useState<'preset' | 'custom'>('preset')

  useEffect(() => {
    if (globalConfig.data) {
      setMetricsEnabled(globalConfig.data.enabled)
      const retention = globalConfig.data.default_retention_days

      setMetricsRetention(retention)
      if ([7, 14, 30, 60, 90].includes(retention)) {
        setMetricsRetentionMode('preset')
      } else {
        setMetricsRetentionMode('custom')
      }
    }
  }, [globalConfig.data])

  const handleSaveMetrics = async (enabled: boolean, retention: number) => {
    await updateGlobalMutation.mutateAsync({ enabled, retentionDays: retention })
  }

  const handleEnabledChange = async (checked: boolean) => {
    setMetricsEnabled(checked)
    await handleSaveMetrics(checked, metricsRetention)
  }

  const handleRetentionChange = async (val: string | null) => {
    if (val === 'custom') {
      setMetricsRetentionMode('custom')
    } else {
      setMetricsRetentionMode('preset')
      const retention = Number(val) || 7

      setMetricsRetention(retention)
      await handleSaveMetrics(metricsEnabled, retention)
    }
  }

  const handleCustomRetentionChange = async (val: number | string) => {
    const retention = Number(val) || 7

    setMetricsRetention(retention)
    await handleSaveMetrics(metricsEnabled, retention)
  }

  const isLoading = globalConfig.isLoading || updateGlobalMutation.isPending

  return (
    <Box>
      <Text size="lg" fw={600} mb="lg">Metrics</Text>

      <Stack gap="md">
        <Card withBorder padding="md" radius="md">
          <Stack gap="md">
            <Switch
              label="Enable metrics by default"
              checked={metricsEnabled}
              onChange={(e) => handleEnabledChange(e.currentTarget.checked)}
              disabled={isLoading}
            />

            <Divider />

            <Select
              label="Retention period"
              value={metricsRetentionMode === 'preset' ? metricsRetention.toString() : 'custom'}
              onChange={handleRetentionChange}
              data={RETENTION_OPTIONS}
              disabled={isLoading}
            />

            {metricsRetentionMode === 'custom' && (
              <NumberInput
                label="Custom days"
                value={metricsRetention}
                onChange={handleCustomRetentionChange}
                min={1}
                max={90}
                disabled={isLoading}
              />
            )}
          </Stack>
        </Card>
      </Stack>
    </Box>
  )
}
