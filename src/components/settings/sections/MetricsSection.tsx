import { Alert, Box, Card, Divider, NumberInput, Select, Stack, Switch, Text } from '@mantine/core'
import { type ReactNode, startTransition, useActionState, useId, useOptimistic, useRef, useState } from 'react'

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

type RetentionMode = 'preset' | 'custom'

interface RetentionActionState {
  error: string | null
  forDraft: string | null
}

const IDLE_STATE: RetentionActionState = { error: null, forDraft: null }

const MetricsForm = ({ config }: { config: GlobalMetricsConfig }): ReactNode => {
  const update = useUpdateGlobalMetricsConfig()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const confirmed = config.default_retention_days
  const [optimisticEnabled, setOptimisticEnabled] = useOptimistic(config.enabled)
  const [mode, setMode] = useState<RetentionMode>(() => (RETENTION_DAYS.includes(confirmed) ? 'preset' : 'custom'))
  const [draft, setDraft] = useState<string | number | null>(null)
  const [touched, setTouched] = useState(false)
  const [commandError, setCommandError] = useState<string | null>(null)

  const draftValue = draft ?? confirmed
  const validationError = touched ? validateRetentionDays(draftValue) : null

  const [retentionState, retentionAction, savingRetention] = useActionState(async (): Promise<RetentionActionState> => {
    const submitted = draftValue
    const submittedKey = String(submitted)

    setTouched(true)

    const validation = validateRetentionDays(submitted)

    if (validation) {
      return { error: validation, forDraft: submittedKey }
    }

    const days = Number(submitted)

    if (days === confirmed) {
      return IDLE_STATE
    }

    try {
      await update.mutateAsync({ enabled: config.enabled, retentionDays: days })
      setCommandError(null)
      setDraft((current) => (current !== null && String(current) === submittedKey ? null : current))

      return IDLE_STATE
    } catch (error) {
      return {
        error: errorMessage(error, 'Try saving the retention period again.'),
        forDraft: submittedKey,
      }
    }
  }, IDLE_STATE)

  const isSaving = update.isPending || savingRetention
  const actionError = retentionState.forDraft === String(draftValue) ? retentionState.error : null
  const retentionError = validationError ?? actionError ?? commandError
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
    if (value === 'custom') {
      setMode('custom')

      return
    }

    setMode('preset')

    const retention = Number(value)

    if (isSaving || validateRetentionDays(retention)) {
      return
    }

    startTransition(async () => {
      try {
        await update.mutateAsync({ enabled: config.enabled, retentionDays: retention })
        setDraft(null)
        setCommandError(null)
      } catch (error) {
        setCommandError(errorMessage(error, 'Try saving the retention period again.'))
      }
    })
  }

  const submitRetention = (): void => {
    setTouched(true)

    if (!isSaving) {
      formRef.current?.requestSubmit()
    }
  }

  const retentionValue = mode === 'preset' ? confirmed.toString() : 'custom'

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <LiveStatus message={retentionError ?? ''} />
        <form id={formId} ref={formRef} action={retentionAction} noValidate hidden />

        <IosGroup>
          <SwitchRow
            title="Enable metrics by default"
            checked={optimisticEnabled}
            onChange={(event) => handleEnabledChange(event.currentTarget.checked)}
            disabled={isSaving}
          />
        </IosGroup>

        <IosGroup
          footer={
            retentionError && !customMode ? (
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
                value={retentionValue}
                onChange={handleRetentionChange}
                data={RETENTION_PRESETS}
                disabled={isSaving}
              />
            )}
          </FieldRow>
          {customMode && (
            <FieldRow title="Custom days">
              {(id) => (
                <RowNumberInput
                  id={id}
                  form={formId}
                  name="retention_days"
                  value={draftValue}
                  onChange={setDraft}
                  onBlur={submitRetention}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault()
                      submitRetention()
                    }
                  }}
                  error={retentionError ?? undefined}
                  min={1}
                  max={90}
                  allowDecimal={false}
                  clampBehavior="none"
                  required
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
          <form id={formId} ref={formRef} action={retentionAction} noValidate>
            <NumberInput
              name="retention_days"
              label="Custom days"
              value={draftValue}
              onChange={setDraft}
              onBlur={submitRetention}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault()
                  submitRetention()
                }
              }}
              error={retentionError ?? undefined}
              min={1}
              max={90}
              allowDecimal={false}
              clampBehavior="none"
              required
            />
          </form>
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

export const MetricsSection = (): ReactNode => {
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
