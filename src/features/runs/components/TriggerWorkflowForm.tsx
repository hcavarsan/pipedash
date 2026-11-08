import { Button, Checkbox, Group, Stack } from '@mantine/core'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'
import { ActionTargetRows } from '../../../components/common/ActionTargetRows'
import { StandardModal } from '../../../components/common/StandardModal'
import { LiveStatus, useErrorAnnouncement } from '../../../components/forms/LiveStatus'
import { useFormEnterNavigation } from '../../../hooks/useFormEnterNavigation'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { usePlugins } from '../../../queries/usePluginsQueries'
import { service } from '../../../services/service'
import { useNotificationStore } from '../../../stores/notificationStore'
import type { Pipeline, TriggerDraft, TriggerResult, WorkflowParameter } from '../../../types/pipelines'
import { displayWarningNotification } from '../../../utils/errorDisplay'
import { useFocusFirstInvalid } from '../../../utils/formFocus'
import { getPluginByType } from '../../../utils/pluginHelpers'
import { changePreference } from '../../notifications/queries/changePreference'
import { comparable, type ParamValues } from '../lib/triggerParams'
import { usePreparedTrigger, useTriggerWorkflow } from '../queries/useWorkflowQueries'
import { TriggerWorkflowBody } from './TriggerWorkflowBody'
import { WorkflowParameterField } from './WorkflowParameterField'

interface DraftState {
  values: ParamValues
  baseline: ParamValues
}

const EMPTY_PARAMETERS: WorkflowParameter[] = []
const EMPTY_VALUES: ParamValues = {}

interface TriggerWorkflowFormProps {
  onClose: () => void
  pipeline: Pipeline
  onSuccess?: ((pipelineId: string, runNumber: number) => void) | undefined
  rerunOf?: number | undefined
}

export function TriggerWorkflowForm({ onClose, pipeline, onSuccess, rerunOf }: TriggerWorkflowFormProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const focusFirstInvalid = useFocusFirstInvalid(formRef)
  useFormEnterNavigation(formRef)
  const [errors, setErrors] = useState<Record<string, string>>({})
  const announcement = useErrorAnnouncement(errors)
  const triggerMutation = useTriggerWorkflow()
  const { data: plugins } = usePlugins()
  const asyncTrigger = !!getPluginByType(plugins, pipeline.provider_type)?.capabilities.async_trigger
  const notifyTriggeredRuns = useNotificationStore((state) => state.notifyTriggeredRuns)
  const setNotifyTriggeredRuns = useNotificationStore((state) => state.setNotifyTriggeredRuns)
  const watched = useNotificationStore((state) => state.pipelineStates[pipeline.id] === 'watching')
  const {
    data,
    error: fetchError,
    isFetching: refetchingParams,
    refetch: refetchParams,
  } = usePreparedTrigger(pipeline.id, rerunOf)
  const [draft, setDraft] = useState<DraftState | null>(null)

  if (draft === null && data) {
    setDraft({ values: data.values, baseline: data.values })
  }

  const copy = data?.copy
  const blockedReason = data?.availability.reason ?? undefined
  const parameters = data?.parameters ?? EMPTY_PARAMETERS
  const paramValues = draft?.values ?? EMPTY_VALUES
  const baseline = draft?.baseline ?? EMPTY_VALUES

  const [, formAction, isPending] = useActionState(async (): Promise<null> => {
    if (!copy) {
      return null
    }

    let checked: TriggerDraft
    let result: TriggerResult

    try {
      checked = await service.preparePipelineTrigger(pipeline.id, rerunOf ?? null, paramValues)
    } catch {
      return null
    }

    setErrors(checked.errors)

    if (Object.keys(checked.errors).length > 0) {
      focusFirstInvalid()

      return null
    }

    try {
      result = await triggerMutation.mutateAsync({
        params: {
          workflow_id: pipeline.id,
          inputs: Object.keys(paramValues).length > 0 ? paramValues : undefined,
        },
        copy,
      })
    } catch {
      return null
    }

    const runNumber = result.run_number ?? 0

    onClose()

    if (notifyTriggeredRuns && !watched) {
      void changePreference({ type: 'watch_next_finish', pipeline_id: pipeline.id })
    }

    if (runNumber > 0) {
      onSuccess?.(pipeline.id, runNumber)
    } else if (!asyncTrigger) {
      displayWarningNotification(
        'The workflow was triggered. Check the pipeline list for the new run.',
        'Run number not available yet'
      )
    }

    return null
  }, null)

  const dirty = parameters.some((param) => comparable(paramValues[param.name]) !== comparable(baseline[param.name]))
  const target = (data?.fields ?? []).filter(({ label }) => mobileShell || label !== 'Pipeline')
  const lastTextName = [...parameters]
    .reverse()
    .find((param) => param.type === 'string' || param.type === 'number')?.name

  const changeValue = (param: WorkflowParameter, value: unknown) => {
    setDraft((current) => current && { ...current, values: { ...current.values, [param.name]: value } })

    setErrors(({ [param.name]: _removed, ...rest }) => rest)
  }

  const triggerDisabled = !draft || !!fetchError || !!blockedReason
  const submitLabel = copy?.label

  const footer = (close: () => void) => (
    <Group justify="flex-end" gap="sm" grow={isMobile}>
      <Button variant="default" size="sm" onClick={close} disabled={isPending}>
        Cancel
      </Button>
      <Button size="sm" type="submit" form={formId} loading={isPending} disabled={triggerDisabled}>
        {submitLabel}
      </Button>
    </Group>
  )

  const fields = parameters.map((param) => (
    <WorkflowParameterField
      key={param.name}
      param={param}
      value={paramValues[param.name]}
      error={errors[param.name]}
      formId={formId}
      last={param.name === lastTextName}
      mobileShell={mobileShell}
      pending={isPending}
      onChange={(value) => changeValue(param, value)}
    />
  ))

  const copyTitle = mobileShell ? copy?.label : `${copy?.title}: ${pipeline.name}`
  const title = copy ? copyTitle : ''

  return (
    <StandardModal
      opened
      onClose={onClose}
      size="md"
      title={title}
      loading={isPending}
      dirty={dirty}
      footer={footer}
      primaryAction={{
        label: copy?.compact_label ?? '',
        onClick: () => formRef.current?.requestSubmit(),
        loading: isPending,
        disabled: triggerDisabled,
      }}
    >
      <form ref={formRef} id={formId} noValidate action={formAction}>
        <Stack mb="md">
          <ActionTargetRows rows={target} />
        </Stack>
        <TriggerWorkflowBody
          fetchError={fetchError}
          retrying={refetchingParams}
          onRetry={() => {
            refetchParams().catch(() => undefined)
          }}
          ready={!!draft}
          blockedReason={blockedReason}
          note={copy?.note}
          parameterCount={parameters.length}
          mobileShell={mobileShell}
          isMobile={isMobile}
          fields={fields}
        />
        {draft && !fetchError && !watched && (
          <Checkbox
            mt="md"
            label="Notify me when it finishes"
            checked={notifyTriggeredRuns}
            disabled={isPending}
            onChange={(event) => setNotifyTriggeredRuns(event.currentTarget.checked)}
          />
        )}
        <LiveStatus message={announcement} />
      </form>
    </StandardModal>
  )
}
