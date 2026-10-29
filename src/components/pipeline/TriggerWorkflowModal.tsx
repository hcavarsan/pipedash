import { Button, Group, NativeSelect, NumberInput, Select, Stack, Switch, Text, TextInput } from '@mantine/core'
import { type ChangeEvent, useEffect, useId, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useTriggerWorkflow, useWorkflowParameters } from '../../queries/useWorkflowQueries'
import type { Pipeline, WorkflowParameter } from '../../types'
import { actionTarget } from '../../utils/actionTarget'
import { displayWarningNotification } from '../../utils/errorDisplay'
import { errorId, fieldA11y, fieldAttributes, helpId, withoutType } from '../../utils/formFields'
import { handleFormEnter, useFocusFirstInvalid } from '../../utils/formFocus'
import { getPipelineLabels, getTriggerBlockReason } from '../../utils/pipelinePresentation'
import { ActionTargetRows } from '../common/ActionTargetRows'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { LiveStatus, useErrorAnnouncement } from '../forms/LiveStatus'
import { IosGroup, IosRow } from '../ios/IosList'
import shell from '../provider/AddProviderModal/ShellForm.module.css'

interface TriggerWorkflowModalProps {
  opened: boolean
  onClose: () => void
  pipeline: Pipeline
  onSuccess?: ((pipelineId: string, runNumber: number) => void) | undefined
  initialInputs?: Record<string, unknown> | undefined
}

const buildInitialParamValues = (params: WorkflowParameter[], inputs?: Record<string, unknown>) => {
  const initialValues: Record<string, unknown> = {}
  const isReplay = inputs !== undefined && Object.keys(inputs).length > 0

  params.forEach((param) => {
    if (isReplay && inputs && inputs[param.name] !== undefined) {
      initialValues[param.name] = inputs[param.name]
    } else if (param.default !== undefined && param.default !== null) {
      if (param.type === 'string' || param.type === 'choice') {
        const defaultStr = String(param.default)

        if (defaultStr.trim() !== '') {
          initialValues[param.name] = param.default
        }
      } else {
        initialValues[param.name] = param.default
      }
    } else if (param.type === 'boolean') {
      initialValues[param.name] = false
    }
  })

  return initialValues
}

const comparable = (value: unknown): string => (value === undefined || value === null ? '' : String(value))

const paramError = (param: WorkflowParameter, value: unknown): string | null => {
  const missing = value === undefined || value === null || (typeof value === 'string' && value.trim() === '')

  return param.required && missing ? `${param.label || param.name} is required` : null
}

export const TriggerWorkflowModal = ({
  opened,
  onClose,
  pipeline,
  onSuccess,
  initialInputs,
}: TriggerWorkflowModalProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const focusFirstInvalid = useFocusFirstInvalid(formRef)
  const [paramValues, setParamValues] = useState<Record<string, unknown>>({})
  const [baseline, setBaseline] = useState<Record<string, unknown>>({})
  const [errors, setErrors] = useState<Record<string, string>>({})
  const announcement = useErrorAnnouncement(errors)
  const isSubmittingRef = useRef(false)
  const labels = getPipelineLabels(pipeline.provider_type)
  const isFlux = pipeline.provider_type === 'fluxcd'
  const blockedReason = getTriggerBlockReason(pipeline)

  const {
    data: parameters = [],
    isLoading: fetchingParams,
    isFetching: refetchingParams,
    error: fetchError,
    refetch: refetchParams,
  } = useWorkflowParameters(pipeline.provider_id, opened ? pipeline.id : '')
  const triggerMutation = useTriggerWorkflow()

  useEffect(() => {
    if (!opened) {
      setParamValues({})
      setBaseline({})
      setErrors({})

      return
    }

    if (parameters.length > 0) {
      const initial = buildInitialParamValues(parameters, initialInputs)

      setParamValues(initial)
      setBaseline(initial)
    }
  }, [opened, parameters, initialInputs])

  const dirty = parameters.some((param) => comparable(paramValues[param.name]) !== comparable(baseline[param.name]))
  const chosenBranch = comparable(paramValues.ref ?? paramValues.branch)
  const target = actionTarget(pipeline, [{ branch: chosenBranch || pipeline.branch, inputs: paramValues }]).filter(
    ({ label }) => mobileShell || label !== 'Pipeline'
  )
  const lastTextName = [...parameters]
    .reverse()
    .find((param) => param.type === 'string' || param.type === 'number')?.name

  const updateErrors = (param: WorkflowParameter, value: unknown) => {
    const error = paramError(param, value)

    setErrors((prev) => {
      const { [param.name]: _removed, ...rest } = prev

      return error ? { ...rest, [param.name]: error } : rest
    })
  }

  const setValue = (param: WorkflowParameter, value: unknown) => {
    setParamValues({ ...paramValues, [param.name]: value })

    if (errors[param.name]) {
      updateErrors(param, value)
    }
  }

  const handleTrigger = () => {
    if (isSubmittingRef.current || triggerMutation.isPending) {
      return
    }

    const nextErrors: Record<string, string> = {}

    for (const param of parameters) {
      const error = paramError(param, paramValues[param.name])

      if (error) {
        nextErrors[param.name] = error
      }
    }

    setErrors(nextErrors)

    if (Object.keys(nextErrors).length > 0) {
      focusFirstInvalid()

      return
    }

    isSubmittingRef.current = true

    triggerMutation.mutate(
      {
        workflow_id: pipeline.id,
        inputs: Object.keys(paramValues).length > 0 ? paramValues : undefined,
      },
      {
        onSuccess: (result) => {
          isSubmittingRef.current = false
          let runNumber = 0

          try {
            const parsed = JSON.parse(result)

            runNumber = parsed.run_number || parsed.build_number || parsed.number || 0
          } catch {
            /* parse error expected for non-JSON responses */
          }

          onClose()

          if (runNumber > 0) {
            onSuccess?.(pipeline.id, runNumber)
          } else if (!isFlux) {
            displayWarningNotification(
              'The workflow was triggered. Check the pipeline list for the new run.',
              'Run number not available yet'
            )
          }
        },
        onError: () => {
          isSubmittingRef.current = false
        },
      }
    )
  }

  const attributesFor = (param: WorkflowParameter) =>
    fieldAttributes({
      name: param.name,
      label: param.label,
      kind: param.type === 'number' ? 'number' : 'text',
      defaultValue: param.default,
      last: param.name === lastTextName,
    })

  const inputProps = (param: WorkflowParameter) => ({
    disabled: triggerMutation.isPending,
    onBlur: () => updateErrors(param, paramValues[param.name]),
  })

  const booleanProps = (param: WorkflowParameter) => ({
    checked: Boolean(paramValues[param.name]),
    onChange: (event: ChangeEvent<HTMLInputElement>) => setValue(param, event.currentTarget.checked),
    disabled: triggerMutation.isPending,
  })

  const choiceProps = (param: WorkflowParameter) => {
    const options = param.options ? [...new Set(param.options)] : []
    const value = paramValues[param.name]

    return {
      ...inputProps(param),
      data: options,
      value: typeof value === 'string' && value !== '' && options.includes(value) ? value : null,
      onChange: (value: string | null) => setValue(param, value),
      clearable: !param.required,
    }
  }

  const numberProps = (param: WorkflowParameter) => {
    const value = paramValues[param.name]

    return {
      ...inputProps(param),
      ...withoutType(attributesFor(param)),
      value: typeof value === 'number' || typeof value === 'string' ? value : '',
      onChange: (value: string | number) => setValue(param, value),
    }
  }

  const textProps = (param: WorkflowParameter) => ({
    ...inputProps(param),
    ...attributesFor(param),
    value: paramValues[param.name] ? String(paramValues[param.name]) : '',
    onChange: (event: ChangeEvent<HTMLInputElement>) => setValue(param, event.currentTarget.value),
  })

  const renderShellRow = (param: WorkflowParameter) => {
    const label = param.label || param.name
    const id = `${formId}-${param.name}`
    const error = errors[param.name]
    const a11y = fieldA11y(id, { required: param.required, error, description: param.description })
    const title = (
      <>
        {label}
        {param.required && (
          <Text component="span" inherit c="red" aria-hidden>
            {' *'}
          </Text>
        )}
      </>
    )
    const subtitle =
      error || param.description ? (
        <>
          {error && (
            <span id={errorId(id)} className={`${shell.line} ${shell.error}`}>
              {error}
            </span>
          )}
          {param.description && (
            <span id={helpId(id)} className={shell.line}>
              {param.description}
            </span>
          )}
        </>
      ) : undefined
    const placeholder = param.required ? 'Required' : 'Optional'
    const rightAligned = {
      input: {
        minHeight: '2em',
        paddingInline: 0,
        background: 'transparent',
        textAlign: 'right' as const,
      },
    }

    switch (param.type) {
      case 'boolean':
        return (
          <IosRow
            key={param.name}
            title={title}
            subtitle={subtitle}
            accessory={<Switch aria-label={label} {...booleanProps(param)} />}
          />
        )

      case 'choice':
        return (
          <IosRow
            key={param.name}
            title={title}
            subtitle={subtitle}
            accessory={
              <NativeSelect
                aria-label={label}
                {...a11y}
                {...inputProps(param)}
                variant="unstyled"
                data={[{ value: '', label: param.required ? 'Select' : 'None' }, ...new Set(param.options ?? [])]}
                value={typeof paramValues[param.name] === 'string' ? String(paramValues[param.name]) : ''}
                onChange={(event) => setValue(param, event.currentTarget.value || null)}
                w="8em"
                rightSectionWidth={20}
                styles={{ input: { ...rightAligned.input, paddingRight: '1.4em' } }}
              />
            }
          />
        )

      case 'number':
        return (
          <IosRow
            key={param.name}
            title={title}
            subtitle={subtitle}
            accessory={
              <NumberInput
                aria-label={label}
                {...a11y}
                {...numberProps(param)}
                variant="unstyled"
                hideControls
                placeholder={placeholder}
                w="6em"
                styles={rightAligned}
              />
            }
          />
        )

      default:
        return (
          <IosRow
            key={param.name}
            title={title}
            subtitle={subtitle}
            accessory={
              <TextInput
                aria-label={label}
                {...a11y}
                {...textProps(param)}
                variant="unstyled"
                placeholder={placeholder}
                w="8em"
                styles={rightAligned}
              />
            }
          />
        )
    }
  }

  const renderParameterInput = (param: WorkflowParameter) => {
    const label = param.label || param.name
    const error = errors[param.name]

    switch (param.type) {
      case 'boolean':
        return (
          <Switch
            key={param.name}
            label={label}
            description={param.description || undefined}
            {...booleanProps(param)}
          />
        )

      case 'choice':
        return (
          <Select
            key={param.name}
            label={label}
            description={param.description || undefined}
            placeholder="Select an option"
            {...choiceProps(param)}
            error={error}
            required={param.required}
          />
        )

      case 'number':
        return (
          <NumberInput
            key={param.name}
            {...numberProps(param)}
            label={label}
            description={param.description || undefined}
            placeholder="Enter a number"
            error={error}
            required={param.required}
          />
        )

      default:
        return (
          <TextInput
            key={param.name}
            {...textProps(param)}
            label={label}
            description={param.description || undefined}
            placeholder={`Enter ${label.toLowerCase()}`}
            error={error}
            required={param.required}
          />
        )
    }
  }

  const isRerun = initialInputs !== undefined
  const triggerDisabled = fetchingParams || !!fetchError || !!blockedReason

  const footer = (close: () => void) => (
    <Group justify="flex-end" gap="sm" grow={isMobile}>
      <Button variant="default" size="sm" onClick={close} disabled={triggerMutation.isPending}>
        Cancel
      </Button>
      <Button size="sm" type="submit" form={formId} loading={triggerMutation.isPending} disabled={triggerDisabled}>
        {isFlux ? labels.trigger : isRerun ? 'Rerun workflow' : 'Trigger workflow'}
      </Button>
    </Group>
  )

  const renderContent = () => {
    if (fetchingParams) {
      return <LoadingState message="Loading workflow parameters..." />
    }

    if (fetchError) {
      return (
        <ErrorState
          title="Couldn't load workflow parameters"
          error={fetchError}
          hint="The workflow can't be triggered until its parameters load."
          onRetry={() => refetchParams()}
          retrying={refetchingParams}
        />
      )
    }
    if (isFlux) {
      return (
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
      )
    }

    if (parameters.length === 0) {
      return (
        <Text size="sm" c="dimmed">
          This workflow has no parameters. It runs with its defaults.
        </Text>
      )
    }

    if (mobileShell) {
      return <IosGroup>{parameters.map((param) => renderShellRow(param))}</IosGroup>
    }

    return <Stack gap={isMobile ? 'xs' : 'sm'}>{parameters.map((param) => renderParameterInput(param))}</Stack>
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      size="md"
      title={
        mobileShell
          ? isFlux
            ? 'Reconcile'
            : isRerun
              ? 'Rerun workflow'
              : 'Run workflow'
          : isFlux
            ? `Reconcile: ${pipeline.name}`
            : isRerun
              ? `Rerun: ${pipeline.name}`
              : `Trigger: ${pipeline.name}`
      }
      loading={triggerMutation.isPending}
      dirty={dirty}
      footer={footer}
      primaryAction={{
        label: isFlux ? labels.trigger : isRerun ? 'Rerun' : 'Trigger',
        onClick: handleTrigger,
        loading: triggerMutation.isPending,
        disabled: triggerDisabled,
      }}
    >
      <form
        ref={formRef}
        id={formId}
        noValidate
        onSubmit={(event) => {
          event.preventDefault()
          handleTrigger()
        }}
        onKeyDown={handleFormEnter}
      >
        <Stack mb="md">
          <ActionTargetRows rows={target} />
        </Stack>
        {renderContent()}
        <LiveStatus message={announcement} />
      </form>
    </StandardModal>
  )
}
