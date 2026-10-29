import { Card, NumberInput, Select, Stack, Switch, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { RETENTION_PRESETS } from '../../constants/metricsRetention'
import { useMobileShell } from '../../hooks/useIsMobile'
import { IosGroup, IosRow } from '../ios/IosList'
import { SwitchRow } from '../settings/mobileParts'

import { MetricsPickerRow } from './MetricsPickerRow'
import classes from './metrics.module.css'
import type { MetricsConfigScope } from './metricsConfigScope'

export type RetentionMode = 'preset' | 'custom'

interface MetricsCollectionFieldsProps {
  scope: MetricsConfigScope
  enabled: boolean
  onEnabledChange: (enabled: boolean) => void
  retentionDraft: string | number
  onRetentionChange: (value: string | number) => void
  retentionMode: RetentionMode
  onRetentionModeChange: (mode: RetentionMode) => void
  retentionError: string | null
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
  retentionError,
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

        <IosGroup footer={retentionMode === 'custom' ? 'Enter 1 to 90 days.' : 'How long to keep metrics data.'}>
          <MetricsPickerRow
            label="Retention period"
            value={retentionMode === 'preset' ? String(retentionDraft) : 'custom'}
            onChange={handlePreset}
            data={RETENTION_PRESETS}
            disabled={retentionLocked}
          />
          {retentionMode === 'custom' && (
            <IosRow
              title="Days"
              invalid={retentionError !== null}
              accessory={
                <NumberInput
                  variant="unstyled"
                  hideControls
                  aria-label="Custom retention (days)"
                  classNames={{ root: classes.numberRoot, input: classes.numberInput }}
                  value={retentionDraft}
                  onChange={onRetentionChange}
                  error={retentionError}
                  allowDecimal={false}
                  clampBehavior="none"
                  min={1}
                  max={90}
                  disabled={retentionLocked}
                />
              }
            />
          )}
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
          <NumberInput
            label="Custom retention (days)"
            description="Enter 1 to 90 days"
            value={retentionDraft}
            onChange={onRetentionChange}
            error={retentionError}
            allowDecimal={false}
            clampBehavior="none"
            min={1}
            max={90}
            disabled={retentionLocked}
          />
        )}
      </Stack>
    </Card>
  )
}
