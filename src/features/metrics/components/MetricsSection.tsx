import { Alert, Card, Divider, Select, Stack, Switch, Text } from '@mantine/core'
import { type ReactNode, startTransition, useOptimistic, useState } from 'react'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { IosGroup } from '../../../components/ios/IosList'
import { FieldRow, RowSelect, SwitchRow } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNumberDraft } from '../../../hooks/useNumberDraft'
import type { GlobalMetricsConfig } from '../../../types/metrics'
import { errorMessage } from '../../../utils/errorDisplay'
import { RETENTION_PRESETS } from '../lib/metricsRetention'
import { useGlobalMetricsConfig, useUpdateGlobalMetricsConfig } from '../queries/useMetricsQueries'
import { RetentionDaysInput } from './RetentionDaysInput'

const RETENTION_DAYS = RETENTION_PRESETS.filter((option) => option.value !== 'custom').map((option) =>
  Number(option.value)
)

type RetentionMode = 'preset' | 'custom'

const MetricsForm = ({ config }: { config: GlobalMetricsConfig }): ReactNode => {
  const update = useUpdateGlobalMetricsConfig()
  const mobileShell = useMobileShell()
  const confirmed = config.default_retention_days
  const [optimisticEnabled, setOptimisticEnabled] = useOptimistic(config.enabled)
  const [mode, setMode] = useState<RetentionMode>(() => (RETENTION_DAYS.includes(confirmed) ? 'preset' : 'custom'))
  const [commandError, setCommandError] = useState<string | null>(null)
  const retention = useNumberDraft({
    saved: confirmed,
    min: 1,
    save: async (days) => {
      await update.mutateAsync({ enabled: config.enabled, retentionDays: days })
      setCommandError(null)
    },
  })

  const isSaving = update.isPending || retention.pending
  const retentionError = retention.error ?? commandError
  const customMode = mode === 'custom'

  const handleEnabledChange = (checked: boolean): void => {
    if (isSaving) {
      return
    }

    startTransition(async () => {
      setOptimisticEnabled(checked)
      setCommandError(null)

      try {
        await update.mutateAsync({ enabled: checked, retentionDays: confirmed })
      } catch (error) {
        setCommandError(errorMessage(error, 'Try updating metrics settings again.'))
      }
    })
  }

  const handleRetentionChange = (value: string | null): void => {
    setCommandError(null)

    if (value === 'custom') {
      setMode('custom')

      return
    }

    setMode('preset')

    const retention = Number(value)

    if (isSaving) {
      return
    }

    startTransition(async () => {
      try {
        await update.mutateAsync({ enabled: config.enabled, retentionDays: retention })
        setCommandError(null)
      } catch (error) {
        setCommandError(errorMessage(error, 'Try saving the retention period again.'))
      }
    })
  }

  const changeCustomDays = (value: string | number): void => {
    retention.onChange(value)
    setCommandError(null)
  }

  const retentionValue = mode === 'preset' ? confirmed.toString() : 'custom'

  if (mobileShell) {
    const retentionFooter =
      retentionError && !customMode ? (
        <Text span inherit c="red" role="alert">
          {retentionError}
        </Text>
      ) : (
        'Metrics are kept for this many days.'
      )
    const retentionField = (
      <FieldRow title="Retention period">
        {(id) => (
          <RowSelect
            id={id}
            value={retentionValue}
            onChange={handleRetentionChange}
            data={RETENTION_PRESETS}
            disabled={isSaving}
          />
        )}
      </FieldRow>
    )
    const customDaysField = (
      <FieldRow title="Custom days">
        {(id) => (
          <RetentionDaysInput
            row
            id={id}
            name="retention_days"
            value={retention.value}
            onChange={changeCustomDays}
            onBlur={retention.commit}
            onEnter={retention.commit}
            error={retentionError ?? undefined}
            required
            disabled={isSaving}
          />
        )}
      </FieldRow>
    )

    return (
      <Stack gap="lg" pb="md">
        <LiveStatus message={retentionError ?? ''} />

        <IosGroup>
          <SwitchRow
            title="Enable metrics by default"
            checked={optimisticEnabled}
            onChange={(event) => handleEnabledChange(event.currentTarget.checked)}
            disabled={isSaving}
          />
        </IosGroup>

        <IosGroup footer={retentionFooter}>
          {retentionField}
          {customMode && customDaysField}
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
          checked={optimisticEnabled}
          onChange={(event) => handleEnabledChange(event.currentTarget.checked)}
          disabled={isSaving}
        />

        <Divider />

        <Select
          label="Retention period"
          value={retentionValue}
          onChange={handleRetentionChange}
          data={RETENTION_PRESETS}
          disabled={isSaving}
        />

        {customMode && (
          <RetentionDaysInput
            name="retention_days"
            label="Custom days"
            value={retention.value}
            onChange={changeCustomDays}
            onBlur={retention.commit}
            onEnter={retention.commit}
            error={retentionError ?? undefined}
            required
          />
        )}
        {retentionError && !customMode && (
          <Alert color="red" role="alert">
            {retentionError}
          </Alert>
        )}
      </Stack>
    </Card>
  )
}

export const MetricsSection = (): ReactNode => (
  <QueryBoundary
    query={useGlobalMetricsConfig()}
    loading="Loading metrics settings…"
    failure="Couldn't load metrics settings"
  >
    {(config) => <MetricsForm config={config} />}
  </QueryBoundary>
)
