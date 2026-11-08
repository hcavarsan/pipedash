import { Card, Select, Stack, Switch, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { SwitchRow } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { MetricsConfigScope } from '../lib/metricsConfigScope'
import { RETENTION_PRESETS } from '../lib/metricsRetention'
import { MetricsPickerRow } from './MetricsPickerRow'
import classes from './metrics.module.css'
import { RetentionDaysInput } from './RetentionDaysInput'

export type RetentionMode = 'preset' | 'custom'

interface MetricsCollectionFieldsProps {
  scope: MetricsConfigScope
  enabled: boolean
  onEnabledChange: (enabled: boolean) => void
  retentionDraft: string | number
  onRetentionChange: (value: string | number) => void
  retentionMode: RetentionMode
  onRetentionModeChange: (mode: RetentionMode) => void
  disabled: boolean
}

export function MetricsCollectionFields({
  scope,
  enabled,
  onEnabledChange,
  retentionDraft,
  onRetentionChange,
  retentionMode,
  onRetentionModeChange,
  disabled,
}: MetricsCollectionFieldsProps): ReactNode {
  const mobileShell = useMobileShell()
  const retentionLocked = !enabled || disabled

  const handlePreset = (value: string | null) => {
    if (value === 'custom') {
      onRetentionModeChange('custom')
    } else {
      onRetentionModeChange('preset')
      onRetentionChange(value ?? '')
    }
  }

  if (mobileShell) {
    const daysInput = (
      <RetentionDaysInput
        variant="unstyled"
        hideControls
        aria-label="Custom retention (days)"
        classNames={{ root: classes.numberRoot, input: classes.numberInput }}
        value={retentionDraft}
        onChange={onRetentionChange}
        disabled={retentionLocked}
      />
    )

    return (
      <>
        <IosGroup
          footer={
            scope.scope === 'pipeline'
              ? `Collect and store metrics for ${scope.pipelineName}.`
              : 'Enable metrics collection by default for new pipelines.'
          }
        >
          <SwitchRow
            title="Enable metrics"
            checked={enabled}
            onChange={(event) => onEnabledChange(event.currentTarget.checked)}
            disabled={disabled}
          />
        </IosGroup>

        <IosGroup footer="How long to keep metrics data.">
          <MetricsPickerRow
            label="Retention period"
            value={retentionMode === 'preset' ? String(retentionDraft) : 'custom'}
            onChange={handlePreset}
            data={RETENTION_PRESETS}
            disabled={retentionLocked}
          />
          {retentionMode === 'custom' && <IosRow title="Days" accessory={daysInput} />}
        </IosGroup>
      </>
    )
  }

  return (
    <Card p="sm">
      <Stack gap="sm">
        <Text size="sm" fw={600}>
          Collection settings
        </Text>
        <Switch
          label="Enable metrics"
          description={
            scope.scope === 'pipeline'
              ? 'Collect and store metrics for this pipeline'
              : 'Enable metrics collection by default for new pipelines'
          }
          checked={enabled}
          onChange={(event) => onEnabledChange(event.currentTarget.checked)}
          disabled={disabled}
        />

        <Select
          label="Retention period"
          description="How long to keep metrics data"
          value={retentionMode === 'preset' ? String(retentionDraft) : 'custom'}
          onChange={handlePreset}
          data={RETENTION_PRESETS}
          allowDeselect={false}
          disabled={retentionLocked}
        />

        {retentionMode === 'custom' && (
          <RetentionDaysInput
            label="Custom retention (days)"
            description="Enter a number of days"
            value={retentionDraft}
            onChange={onRetentionChange}
            disabled={retentionLocked}
          />
        )}
      </Stack>
    </Card>
  )
}
