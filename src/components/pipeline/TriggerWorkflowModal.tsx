import { Button, Group, NumberInput, Select, Stack, Switch, Text, TextInput } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { useEffect, useRef, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useTriggerWorkflow, useWorkflowParameters } from '../../queries/useWorkflowQueries'
import type { Pipeline, WorkflowParameter } from '../../types'
import { getPipelineLabels, getReconcileBlockReason } from '../../utils/pipelinePresentation'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'
import { IosGroup, IosRow } from '../ios/IosList'

interface TriggerWorkflowModalProps {
  opened: boolean
  onClose: () => void
  pipeline: Pipeline
  onSuccess?: (pipelineId: string, runNumber: number) => void
  initialInputs?: Record<string, unknown>
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

export const TriggerWorkflowModal = ({
  opened,
  onClose,
  pipeline,
  onSuccess,
  initialInputs,
}: TriggerWorkflowModalProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const [paramValues, setParamValues] = useState<Record<string, unknown>>({})
  const isSubmittingRef = useRef(false)
  const labels = getPipelineLabels(pipeline.provider_type)
  const isFlux = pipeline.provider_type === 'fluxcd'
  const blockedReason = getReconcileBlockReason(pipeline)

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

      return
    }

    if (parameters.length > 0) {
      setParamValues(buildInitialParamValues(parameters, initialInputs))
    }
  }, [opened, parameters, initialInputs])

  const handleTrigger = () => {
    if (isSubmittingRef.current || triggerMutation.isPending) {
      return
    }
    isSubmittingRef.current = true

    const missing = parameters.find((param) => {
      const value = paramValues[param.name]

      return (
        param.required && (value === undefined || value === null || (typeof value === 'string' && value.trim() === ''))
      )
    })

    if (missing) {
      notifications.show({
        title: 'Required parameter is empty',
        message: `Enter a value for "${missing.label || missing.name}" to trigger the workflow.`,
        color: 'red',
      })
      isSubmittingRef.current = false

      return
    }

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
            notifications.show({
              title: 'Run number not available yet',
              message: 'The workflow was triggered. Check the pipeline list for the new run.',
              color: 'orange',
              autoClose: 5000,
            })
          }
        },
        onError: () => {
          isSubmittingRef.current = false
        },
      }
    )
  }

  const renderShellRow = (param: WorkflowParameter) => {
    const label = param.label || param.name
    const value = paramValues[param.name]
    const isDisabled = triggerMutation.isPending
    const title = (
      <>
        {label}
        {param.required && (
          <Text component="span" inherit c="red">
            {' *'}
          </Text>
        )}
      </>
    )
    const subtitle = param.description || undefined
    const placeholder = param.required ? 'Required' : 'Optional'
    const rightAligned = {
      input: {
        height: 32,
        minHeight: 32,
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
            accessory={
              <Switch
                aria-label={label}
                checked={Boolean(value)}
                onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.checked })}
                disabled={isDisabled}
              />
            }
          />
        )

      case 'choice': {
        const options = param.options ? [...new Set(param.options)] : []
        const validValue = typeof value === 'string' && value !== '' && options.includes(value) ? value : null

        return (
          <IosRow
            key={param.name}
            title={title}
            subtitle={subtitle}
            accessory={
              <Select
                aria-label={label}
                variant="unstyled"
                placeholder="Select"
                data={options}
                value={validValue}
                onChange={(val) => setParamValues({ ...paramValues, [param.name]: val })}
                clearable={!param.required}
                disabled={isDisabled}
                w={160}
                rightSection={<span />}
                rightSectionPointerEvents="none"
                styles={rightAligned}
              />
            }
          />
        )
      }

      case 'number':
        return (
          <IosRow
            key={param.name}
            title={title}
            subtitle={subtitle}
            accessory={
              <NumberInput
                aria-label={label}
                variant="unstyled"
                hideControls
                placeholder={placeholder}
                value={typeof value === 'number' || typeof value === 'string' ? value : ''}
                onChange={(val) => setParamValues({ ...paramValues, [param.name]: val })}
                disabled={isDisabled}
                w={140}
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
                variant="unstyled"
                placeholder={placeholder}
                value={value ? String(value) : ''}
                onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.value })}
                disabled={isDisabled}
                w={160}
                styles={rightAligned}
              />
            }
          />
        )
    }
  }

  const renderParameterInput = (param: WorkflowParameter) => {
    const label = param.label || param.name
    const value = paramValues[param.name]
    const isDisabled = triggerMutation.isPending

    switch (param.type) {
      case 'boolean':
        return (
          <Switch
            key={param.name}
            label={label}
            description={param.description || undefined}
            checked={Boolean(value)}
            onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.checked })}
            disabled={isDisabled}
          />
        )

      case 'choice': {
        const options = param.options ? [...new Set(param.options)] : []
        const validValue = typeof value === 'string' && value !== '' && options.includes(value) ? value : null

        return (
          <Select
            key={param.name}
            label={label}
            description={param.description || undefined}
            placeholder="Select an option"
            data={options}
            value={validValue}
            onChange={(val) => setParamValues({ ...paramValues, [param.name]: val })}
            required={param.required}
            clearable={!param.required}
            disabled={isDisabled}
          />
        )
      }

      case 'number':
        return (
          <NumberInput
            key={param.name}
            label={label}
            description={param.description || undefined}
            placeholder="Enter a number"
            value={typeof value === 'number' || typeof value === 'string' ? value : ''}
            onChange={(val) => setParamValues({ ...paramValues, [param.name]: val })}
            required={param.required}
            disabled={isDisabled}
          />
        )

      default:
        return (
          <TextInput
            key={param.name}
            label={label}
            description={param.description || undefined}
            placeholder={`Enter ${label.toLowerCase()}`}
            value={value ? String(value) : ''}
            onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.value })}
            required={param.required}
            disabled={isDisabled}
          />
        )
    }
  }

  const isRerun = initialInputs !== undefined

  const footer = (
    <Group justify="flex-end" gap="sm" grow={isMobile}>
      {!mobileShell && (
        <Button variant="default" size="sm" onClick={onClose} disabled={triggerMutation.isPending}>
          Cancel
        </Button>
      )}
      <Button
        size="sm"
        onClick={handleTrigger}
        loading={triggerMutation.isPending}
        disabled={fetchingParams || !!fetchError || !!blockedReason}
      >
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
      title={isFlux ? `Reconcile: ${pipeline.name}` : isRerun ? `Rerun: ${pipeline.name}` : `Trigger: ${pipeline.name}`}
      loading={triggerMutation.isPending}
      footer={footer}
    >
      {renderContent()}
    </StandardModal>
  )
}
