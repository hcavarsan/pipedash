import { Box, Card, Divider, NumberInput, Select, Stack, Switch, Title } from '@mantine/core'
import { useState } from 'react'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { useGlobalMetricsConfig, useUpdateGlobalMetricsConfig } from '../../../queries/useMetricsQueries'
import type { GlobalMetricsConfig } from '../../../types'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'
import { RowNumberInput, RowSelect } from '../mobileParts'

const RETENTION_PRESETS = [7, 14, 30, 60, 90]

const RETENTION_OPTIONS = [
  ...RETENTION_PRESETS.map((days) => ({ value: String(days), label: `${days} days` })),
  { value: 'custom', label: 'Custom' },
]

const MetricsForm = ({ config }: { config: GlobalMetricsConfig }) => {
  const updateGlobalMutation = useUpdateGlobalMetricsConfig()
  const mobileShell = useMobileShell()

  const [metricsEnabled, setMetricsEnabled] = useState(config.enabled)
  const [metricsRetention, setMetricsRetention] = useState(config.default_retention_days)
  const [metricsRetentionMode, setMetricsRetentionMode] = useState<'preset' | 'custom'>(
    RETENTION_PRESETS.includes(config.default_retention_days) ? 'preset' : 'custom'
  )

  const handleEnabledChange = async (checked: boolean) => {
    setMetricsEnabled(checked)
    await updateGlobalMutation.mutateAsync({ enabled: checked, retentionDays: metricsRetention })
  }

  const handleRetentionChange = async (val: string | null) => {
    if (val === 'custom') {
      setMetricsRetentionMode('custom')
    } else {
      setMetricsRetentionMode('preset')
      const retention = Number(val) || 7

      setMetricsRetention(retention)
      await updateGlobalMutation.mutateAsync({ enabled: metricsEnabled, retentionDays: retention })
    }
  }

  const handleCustomRetentionChange = async (val: number | string) => {
    const retention = Number(val) || 7

    setMetricsRetention(retention)
    await updateGlobalMutation.mutateAsync({ enabled: metricsEnabled, retentionDays: retention })
  }

  const isSaving = updateGlobalMutation.isPending

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <IosGroup>
          <IosRow
            title="Enable metrics by default"
            accessory={
              <Switch
                aria-label="Enable metrics by default"
                checked={metricsEnabled}
                onChange={(e) => handleEnabledChange(e.currentTarget.checked)}
                disabled={isSaving}
              />
            }
          />
        </IosGroup>

        <IosGroup>
          <IosRow
            title="Retention period"
            accessory={
              <RowSelect
                aria-label="Retention period"
                value={metricsRetentionMode === 'preset' ? metricsRetention.toString() : 'custom'}
                onChange={handleRetentionChange}
                data={RETENTION_OPTIONS}
                disabled={isSaving}
              />
            }
          />
          {metricsRetentionMode === 'custom' && (
            <IosRow
              title="Custom days"
              accessory={
                <RowNumberInput
                  aria-label="Custom retention days"
                  value={metricsRetention}
                  onChange={handleCustomRetentionChange}
                  min={1}
                  max={90}
                  disabled={isSaving}
                />
              }
            />
          )}
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Switch
          label="Enable metrics by default"
          checked={metricsEnabled}
          onChange={(e) => handleEnabledChange(e.currentTarget.checked)}
          disabled={isSaving}
        />

        <Divider />

        <Select
          label="Retention period"
          value={metricsRetentionMode === 'preset' ? metricsRetention.toString() : 'custom'}
          onChange={handleRetentionChange}
          data={RETENTION_OPTIONS}
          disabled={isSaving}
        />

        {metricsRetentionMode === 'custom' && (
          <NumberInput
            label="Custom days"
            value={metricsRetention}
            onChange={handleCustomRetentionChange}
            min={1}
            max={90}
            disabled={isSaving}
          />
        )}
      </Stack>
    </Card>
  )
}

export const MetricsSection = () => {
  const { data: config, isLoading, error, refetch, isRefetching } = useGlobalMetricsConfig()

  return (
    <Box>
      <Title order={2} size="lg" fw={600} mb="lg">
        Metrics
      </Title>

      {isLoading ? (
        <LoadingState message="Loading metrics settings..." />
      ) : config ? (
        <MetricsForm key={`${config.enabled}-${config.default_retention_days}`} config={config} />
      ) : (
        <ErrorState
          title="Couldn't load metrics settings"
          error={error}
          onRetry={() => refetch()}
          retrying={isRefetching}
        />
      )}
    </Box>
  )
}
