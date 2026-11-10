import { Button, Group, Stack, Text } from '@mantine/core'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { StandardModal } from '../../../components/common/StandardModal'
import { useBlurValidation } from '../../../hooks/useBlurValidation'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { logger } from '../../../lib/logger'
import { Z } from '../../../theme/zIndex'
import { openConfirm } from '../../../utils/confirm'
import { errorMessage } from '../../../utils/errorDisplay'
import { sameJson } from '../../../utils/sameJson'
import { type ConfigSnapshot, type MetricsConfigModalProps, metricsConfigTitle } from '../lib/metricsConfigScope'
import { RETENTION_PRESETS } from '../lib/metricsRetention'
import {
  useFlushMetrics,
  useUpdateGlobalMetricsConfig,
  useUpdatePipelineMetricsConfig,
} from '../queries/useMetricsQueries'
import { MetricsCollectionFields, type RetentionMode } from './MetricsCollectionFields'
import { MetricsStorageSection } from './MetricsStorageSection'
import classes from './metrics.module.css'

type MetricsConfigFormProps = MetricsConfigModalProps & { initial: ConfigSnapshot }

interface SaveState {
  error: string | null
  submitted: { enabled: boolean; retention: string | number } | null
}

const INITIAL_STATE: SaveState = { error: null, submitted: null }

const leaveFormTransition = (): Promise<void> => Promise.resolve()

async function confirmDisable(pipeline: boolean, mobileShell: boolean): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>()

  await leaveFormTransition()
  openConfirm({
    ...(mobileShell ? { zIndex: Z.alert } : {}),
    title: 'Disable metrics',
    children: (
      <Text size="sm">
        Disabling metrics permanently deletes all stored metrics for {pipeline ? 'this pipeline' : 'all pipelines'}. It
        cannot be undone.
      </Text>
    ),
    labels: { confirm: 'Disable and delete data', cancel: 'Cancel' },
    confirmProps: { color: 'red' },
    onConfirm: () => resolve(true),
    onCancel: () => resolve(false),
    onClose: () => resolve(false),
  })

  return promise
}

export function MetricsConfigForm(props: MetricsConfigFormProps): ReactNode {
  const { opened, onClose, onConfigChange, initial } = props
  const pipelineId = props.scope === 'pipeline' ? props.pipelineId : undefined
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const updateGlobal = useUpdateGlobalMetricsConfig()
  const updatePipeline = useUpdatePipelineMetricsConfig()
  const flushMutation = useFlushMetrics({ toastErrors: false })

  const [baseline] = useState(initial)
  const [enabled, setEnabled] = useState(initial.enabled)
  const [retentionMode, setRetentionMode] = useState<RetentionMode>(
    RETENTION_PRESETS.some((option) => option.value === String(initial.retentionDays)) ? 'preset' : 'custom'
  )
  const [retentionDraft, setRetentionDraft] = useState<string | number>(initial.retentionDays)

  const draftNow = { enabled, retention: retentionDraft }
  const validation = useBlurValidation<'retention'>((_key, value) => {
    if (value.trim() === '') {
      return 'Enter a number'
    }

    const days = Number(value)

    return Number.isInteger(days) && days >= 1 ? undefined : 'Must be at least 1'
  })
  const [deleteError, setDeleteError] = useState<string | null>(null)

  const saveAction = async (): Promise<SaveState> => {
    if (!validation.validateAll({ retention: String(retentionDraft) })) {
      return INITIAL_STATE
    }

    const disabling = baseline.enabled && !enabled

    if (disabling && !(await confirmDisable(pipelineId !== undefined, mobileShell))) {
      return INITIAL_STATE
    }

    try {
      const retentionDays = Number(retentionDraft)

      if (pipelineId) {
        await updatePipeline.mutateAsync({ pipelineId, enabled, retentionDays })
      } else {
        await updateGlobal.mutateAsync({ enabled, retentionDays })
      }
    } catch (error) {
      logger.error('MetricsConfigForm', 'Failed to save metrics configuration', error)

      return { error: errorMessage(error, 'Try saving the metrics settings again.'), submitted: draftNow }
    }

    onConfigChange?.()

    if (disabling) {
      try {
        await flushMutation.mutateAsync(pipelineId)
      } catch (error) {
        logger.error('MetricsConfigForm', 'Failed to delete stored metrics', error)
        setDeleteError(errorMessage(error, 'Try deleting the stored metrics again.'))

        return INITIAL_STATE
      }
    }

    onClose()

    return INITIAL_STATE
  }

  const [state, formAction, isPending] = useActionState(saveAction, INITIAL_STATE)

  const changeRetention = (value: string | number): void => {
    setRetentionDraft(value)
    validation.onChange('retention', String(value))
  }

  const retryDelete = async (): Promise<void> => {
    try {
      await flushMutation.mutateAsync(pipelineId)
      setDeleteError(null)
      onClose()
    } catch (error) {
      setDeleteError(errorMessage(error, 'Try deleting the stored metrics again.'))
    }
  }

  const handleFlush = () => {
    openConfirm({
      ...(mobileShell ? { zIndex: Z.alert } : {}),
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

  const isBusy = isPending || flushMutation.isPending
  const dirty =
    deleteError === null && (enabled !== baseline.enabled || Number(retentionDraft) !== baseline.retentionDays)
  const saveDisabled = flushMutation.isPending || deleteError !== null

  const deleteFailure = deleteError && (
    <Stack gap="xs" align="flex-start">
      <FormErrorDisplay title="Couldn't delete metrics" globalError={deleteError} />
      <Button size="xs" variant="default" loading={flushMutation.isPending} onClick={() => void retryDelete()}>
        Retry
      </Button>
    </Stack>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      size="md"
      loading={isBusy}
      dirty={dirty}
      title={metricsConfigTitle(props, mobileShell)}
      footer={(requestClose) => (
        <Group justify="flex-end" gap="sm">
          <Button variant="default" size="sm" onClick={requestClose} disabled={isBusy}>
            Cancel
          </Button>
          <Button type="submit" form={formId} size="sm" loading={isPending} disabled={saveDisabled}>
            Save
          </Button>
        </Group>
      )}
      primaryAction={{
        label: 'Save',
        onClick: () => formRef.current?.requestSubmit(),
        loading: isPending,
        disabled: saveDisabled,
      }}
    >
      <form id={formId} ref={formRef} action={formAction} noValidate>
        <Stack gap={mobileShell ? 'lg' : 'sm'} className={mobileShell ? classes.num : undefined}>
          <FormErrorDisplay
            title="Couldn't save metrics settings"
            globalError={sameJson(state.submitted, draftNow) ? state.error : null}
          />
          {deleteFailure}
          <MetricsCollectionFields
            scope={props}
            enabled={enabled}
            onEnabledChange={setEnabled}
            retentionDraft={retentionDraft}
            onRetentionChange={changeRetention}
            retentionMode={retentionMode}
            onRetentionModeChange={setRetentionMode}
            disabled={isBusy}
            retentionError={validation.errors.retention}
            onRetentionBlur={() => validation.onBlur('retention', String(retentionDraft))}
          />
          <MetricsStorageSection
            scope={props}
            saving={isPending}
            flushing={flushMutation.isPending}
            onFlush={handleFlush}
          />
        </Stack>
      </form>
    </StandardModal>
  )
}
