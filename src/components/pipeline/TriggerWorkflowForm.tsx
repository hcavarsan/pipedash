import { Button, Group, Stack, Text } from '@mantine/core'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'

import { useFormEnterNavigation } from '../../hooks/useFormEnterNavigation'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useTriggerWorkflow, useWorkflowParameters } from '../../queries/useWorkflowQueries'
import type { Pipeline, WorkflowParameter } from '../../types'
import { actionTarget } from '../../utils/actionTarget'
import { displayWarningNotification } from '../../utils/errorDisplay'
import { useFocusFirstInvalid } from '../../utils/formFocus'
import { getPipelineLabels, getTriggerBlockReason } from '../../utils/pipelinePresentation'
import { ActionTargetRows } from '../common/ActionTargetRows'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { LiveStatus, useErrorAnnouncement } from '../forms/LiveStatus'
import { IosGroup } from '../ios/IosList'
import {
  buildInitialParamValues,
  comparable,
  type ParamValues,
  paramError,
  parseRunNumber,
  triggerModalTitle,
  validateParams,
} from './triggerParams'
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
  initialInputs?: Record<string, unknown> | undefined
}

export function TriggerWorkflowForm({
  onClose,
  pipeline,
  onSuccess,
  initialInputs,
}: TriggerWorkflowFormProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const focusFirstInvalid = useFocusFirstInvalid(formRef)
  useFormEnterNavigation(formRef)
  const [errors, setErrors] = useState<Record<string, string>>({})
  const announcement = useErrorAnnouncement(errors)
  const triggerMutation = useTriggerWorkflow()
  const labels = getPipelineLabels(pipeline.provider_type)
  const isFlux = pipeline.provider_type === 'fluxcd'
  const isRerun = initialInputs !== undefined
  const blockedReason = getTriggerBlockReason(pipeline)
  const {
    data,
    error: fetchError,
    isFetching: refetchingParams,
    refetch: refetchParams,
  } = useWorkflowParameters(pipeline.provider_id, pipeline.id)
  const [draft, setDraft] = useState<DraftState | null>(null)

  if (draft === null && data) {
    const initial = buildInitialParamValues(data, initialInputs)

    setDraft({ values: initial, baseline: initial })
  }

  const parameters = data ?? EMPTY_PARAMETERS
  const paramValues = draft?.values ?? EMPTY_VALUES
  const baseline = draft?.baseline ?? EMPTY_VALUES

  const [, formAction, isPending] = useActionState(async (): Promise<null> => {
    const nextErrors = validateParams(parameters, paramValues)

    setErrors(nextErrors)

    if (Object.keys(nextErrors).length > 0) {
      focusFirstInvalid()

      return null
    }

    let result: string

    try {
      result = await triggerMutation.mutateAsync({
        workflow_id: pipeline.id,
        inputs: Object.keys(paramValues).length > 0 ? paramValues : undefined,
      })
    } catch {
      return null
    }

    const runNumber = parseRunNumber(result)

    onClose()

    if (runNumber > 0) {
      onSuccess?.(pipeline.id, runNumber)
    } else if (!isFlux) {
      displayWarningNotification(
        'The workflow was triggered. Check the pipeline list for the new run.',
        'Run number not available yet'
      )
    }

    return null
  }, null)

  const dirty = parameters.some((param) => comparable(paramValues[param.name]) !== comparable(baseline[param.name]))
  const chosenBranch = comparable(paramValues.ref ?? paramValues.branch)
  const target = actionTarget(pipeline, [{ branch: chosenBranch || pipeline.branch, inputs: paramValues }]).filter(
    ({ label }) => mobileShell || label !== 'Pipeline'
  )
  const lastTextName = [...parameters]
    .reverse()
    .find((param) => param.type === 'string' || param.type === 'number')?.name

  const changeValue = (param: WorkflowParameter, value: unknown) => {
    setDraft((current) => current && { ...current, values: { ...current.values, [param.name]: value } })

    if (errors[param.name]) {
      validateField(param, value)
    }
  }

  const validateField = (param: WorkflowParameter, value: unknown) => {
    const error = paramError(param, value)

    setErrors(({ [param.name]: _removed, ...rest }) => (error ? { ...rest, [param.name]: error } : rest))
  }

  const triggerDisabled = !draft || !!fetchError || !!blockedReason
  const submitLabel = isFlux ? labels.trigger : isRerun ? 'Rerun workflow' : 'Trigger workflow'

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
      onBlur={() => validateField(param, paramValues[param.name])}
    />
  ))

  return (
    <StandardModal
      opened
      onClose={onClose}
      size="md"
      title={triggerModalTitle(pipeline, mobileShell, isFlux, isRerun)}
      loading={isPending}
      dirty={dirty}
      footer={footer}
      primaryAction={{
        label: isFlux ? labels.trigger : isRerun ? 'Rerun' : 'Trigger',
        onClick: () => formRef.current?.requestSubmit(),
        loading: isPending,
        disabled: triggerDisabled,
      }}
    >
      <form ref={formRef} id={formId} noValidate action={formAction}>
        <Stack mb="md">
          <ActionTargetRows rows={target} />
        </Stack>
        {fetchError ? (
          <ErrorState
            title="Couldn't load workflow parameters"
            error={fetchError}
            hint="The workflow can't be triggered until its parameters load."
            onRetry={() => {
              refetchParams().catch(() => undefined)
            }}
            retrying={refetchingParams}
          />
        ) : !draft ? (
          <LoadingState message="Loading workflow parameters..." />
        ) : isFlux ? (
          <Stack gap="sm">
            <Text size="sm">
              Reconcile this resource using its current configuration in the cluster. This does not roll back to a
              previous revision.
            </Text>
            <Text size="sm" c={blockedReason ? 'orange' : 'dimmed'}>
              {blockedReason ??
                'Flux processes the request asynchronously. Its status and history update after the controller handles it.'}
            </Text>
          </Stack>
        ) : parameters.length === 0 ? (
          <Text size="sm" c="dimmed">
            This workflow has no parameters. It runs with its defaults.
          </Text>
        ) : mobileShell ? (
          <IosGroup>{fields}</IosGroup>
        ) : (
          <Stack gap={isMobile ? 'xs' : 'sm'}>{fields}</Stack>
        )}
        <LiveStatus message={announcement} />
      </form>
    </StandardModal>
  )
}
