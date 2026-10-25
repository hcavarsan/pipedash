import { Alert, Box, Card, Divider, NumberInput, Select, Stack, Switch, Text } from '@mantine/core'
import { useState } from 'react'

import { RETENTION_PRESETS, validateRetentionDays } from '../../../constants/metricsRetention'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useGlobalMetricsConfig, useUpdateGlobalMetricsConfig } from '../../../queries/useMetricsQueries'
import type { GlobalMetricsConfig } from '../../../types'
import { errorMessage } from '../../../utils/errorDisplay'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { LiveStatus } from '../../forms/LiveStatus'
import { IosGroup } from '../../ios/IosList'
import { FieldRow, RowNumberInput, RowSelect, SwitchRow } from '../mobileParts'

const RETENTION_DAYS = RETENTION_PRESETS.filter((option) => option.value !== 'custom').map((option) =>
  Number(option.value)
)

const MetricsForm = ({ config }: { config: GlobalMetricsConfig }) => {
  const updateGlobalMutation = useUpdateGlobalMetricsConfig()
  const mobileShell = useMobileShell()

  const [metricsEnabled, setMetricsEnabled] = useState(config.enabled)
  const [metricsRetention, setMetricsRetention] = useState(config.default_retention_days)
  const [metricsRetentionMode, setMetricsRetentionMode] = useState<'preset' | 'custom'>(
    RETENTION_DAYS.includes(config.default_retention_days) ? 'preset' : 'custom'
  )
  const [retentionDraft, setRetentionDraft] = useState<string | number>(config.default_retention_days)
  const [retentionTouched, setRetentionTouched] = useState(false)
  const [retentionError, setRetentionError] = useState<string | null>(null)
  const [savedConfig, setSavedConfig] = useState(config)

  if (config.enabled !== savedConfig.enabled || config.default_retention_days !== savedConfig.default_retention_days) {
    setSavedConfig(config)
    setMetricsEnabled(config.enabled)
    if (Number(retentionDraft) === metricsRetention) {
      setMetricsRetention(config.default_retention_days)
      setRetentionDraft(config.default_retention_days)
      setMetricsRetentionMode(RETENTION_DAYS.includes(config.default_retention_days) ? 'preset' : 'custom')
    }
  }

  const saveRetentionDraft = async () => {
    setRetentionTouched(true)
    const validation = validateRetentionDays(retentionDraft)
    setRetentionError(validation)
    if (validation || Number(retentionDraft) === metricsRetention) {
      return
    }
    try {
      await updateGlobalMutation.mutateAsync({ enabled: metricsEnabled, retentionDays: Number(retentionDraft) })
      setMetricsRetention(Number(retentionDraft))
    } catch (error) {
      setRetentionDraft(metricsRetention)
      setRetentionError(errorMessage(error, 'Try saving the retention period again.'))
    }
  }

  const handleEnabledChange = async (checked: boolean) => {
    setMetricsEnabled(checked)
    try {
      await updateGlobalMutation.mutateAsync({ enabled: checked, retentionDays: metricsRetention })
    } catch (error) {
      setMetricsEnabled(!checked)
      setRetentionError(errorMessage(error, 'Try updating metrics settings again.'))
    }
  }

  const handleRetentionChange = async (val: string | null) => {
    if (val === 'custom') {
      setMetricsRetentionMode('custom')
    } else {
      setMetricsRetentionMode('preset')
      const retention = Number(val)
      if (validateRetentionDays(retention)) {
        return
      }
      try {
        await updateGlobalMutation.mutateAsync({ enabled: metricsEnabled, retentionDays: retention })
        setMetricsRetention(retention)
        setRetentionDraft(retention)
        setRetentionError(null)
      } catch (error) {
        setRetentionDraft(metricsRetention)
        setRetentionError(errorMessage(error, 'Try saving the retention period again.'))
      }
    }
  }

  const isSaving = updateGlobalMutation.isPending

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <IosGroup>
          <SwitchRow
            title="Enable metrics by default"
            checked={metricsEnabled}
            onChange={(e) => handleEnabledChange(e.currentTarget.checked)}
            disabled={isSaving}
          />
        </IosGroup>

        <IosGroup
          footer={
            retentionError ? (
              <Text span inherit c="red" role="alert">
                {retentionError}
              </Text>
            ) : (
              'Metrics are kept for this many days.'
            )
          }
        >
          <FieldRow title="Retention period">
            {(id) => (
              <RowSelect
                id={id}
                value={metricsRetentionMode === 'preset' ? metricsRetention.toString() : 'custom'}
                onChange={handleRetentionChange}
                data={RETENTION_PRESETS}
                disabled={isSaving}
              />
            )}
          </FieldRow>
          {metricsRetentionMode === 'custom' && (
            <FieldRow title="Custom days">
              {(id) => (
                <RowNumberInput
                  id={id}
                  value={retentionDraft}
                  onChange={(value) => {
                    setRetentionDraft(value)
                    if (retentionTouched) {
                      setRetentionError(validateRetentionDays(value))
                    }
                  }}
                  onBlur={() => void saveRetentionDraft()}
                  error={Boolean(retentionError)}
                  min={1}
                  max={90}
                  disabled={isSaving}
                />
              )}
            </FieldRow>
          )}
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <LiveStatus message={retentionError ?? ''} />
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
          data={RETENTION_PRESETS}
          disabled={isSaving}
        />

        {metricsRetentionMode === 'custom' && (
          <NumberInput
            label="Custom days"
            value={retentionDraft}
            onChange={(value) => {
              setRetentionDraft(value)
              if (retentionTouched) {
                setRetentionError(validateRetentionDays(value))
              }
            }}
            onBlur={() => {
              void saveRetentionDraft()
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                event.currentTarget.blur()
              }
            }}
            error={retentionError}
            min={1}
            max={90}
            allowDecimal={false}
            clampBehavior="none"
          />
        )}
        {retentionError && metricsRetentionMode !== 'custom' && (
          <Alert color="red" role="alert">
            {retentionError}
          </Alert>
        )}
      </Stack>
    </Card>
  )
}

export const MetricsSection = () => {
  const { data: config, isLoading, error, refetch, isRefetching } = useGlobalMetricsConfig()

  return (
    <Box>
      {isLoading ? (
        <LoadingState message="Loading metrics settings..." />
      ) : config ? (
        <MetricsForm config={config} />
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
