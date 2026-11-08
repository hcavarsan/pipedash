import { Alert, Box, Card, Divider, Select, Stack, Switch, Text } from '@mantine/core'
import { type ReactNode, startTransition, useActionState, useId, useOptimistic, useRef, useState } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { IosGroup } from '../../../components/ios/IosList'
import { FieldRow, RowSelect, SwitchRow } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { GlobalMetricsConfig } from '../../../types/metrics'
import { errorMessage } from '../../../utils/errorDisplay'
import { RETENTION_PRESETS } from '../lib/metricsRetention'
import { useGlobalMetricsConfig, useUpdateGlobalMetricsConfig } from '../queries/useMetricsQueries'
import { RetentionDaysInput } from './RetentionDaysInput'

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
  const [commandError, setCommandError] = useState<string | null>(null)

  const draftValue = draft ?? confirmed

  const [retentionState, retentionAction, savingRetention] = useActionState(async (): Promise<RetentionActionState> => {
    const submitted = draftValue
    const submittedKey = String(submitted)

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
  const retentionError = actionError ?? commandError
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

    if (isSaving) {
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
    if (!isSaving) {
      formRef.current?.requestSubmit()
    }
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
            form={formId}
            name="retention_days"
            value={draftValue}
            onChange={setDraft}
            onBlur={submitRetention}
            onEnter={submitRetention}
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
        <form id={formId} ref={formRef} action={retentionAction} noValidate hidden />

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
          <form id={formId} ref={formRef} action={retentionAction} noValidate>
            <RetentionDaysInput
              name="retention_days"
              label="Custom days"
              value={draftValue}
              onChange={setDraft}
              onBlur={submitRetention}
              onEnter={submitRetention}
              error={retentionError ?? undefined}
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

  let content: ReactNode
  if (isLoading) {
    content = <LoadingState message="Loading metrics settings..." />
  } else if (config) {
    content = <MetricsForm config={config} />
  } else {
    content = (
      <ErrorState
        title="Couldn't load metrics settings"
        error={error}
        onRetry={() => refetch()}
        retrying={isRefetching}
      />
    )
  }

  return <Box>{content}</Box>
}
