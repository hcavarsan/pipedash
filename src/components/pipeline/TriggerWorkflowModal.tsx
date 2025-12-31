import { useEffect, useRef, useState } from 'react'

import { Alert, Button, Loader, NumberInput, Select, Stack, Switch, Text, TextInput } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useTriggerWorkflow, useWorkflowParameters } from '../../queries/useWorkflowQueries'
import type { Pipeline, WorkflowParameter } from '../../types'
import { displayErrorNotification } from '../../utils/errorDisplay'
import { StandardModal } from '../common/StandardModal'

interface TriggerWorkflowModalProps {
  opened: boolean;
  onClose: () => void;
  pipeline: Pipeline;
  onSuccess?: (pipelineId: string, runNumber: number) => void;
  initialInputs?: Record<string, any>;
}

export const TriggerWorkflowModal = ({
  opened,
  onClose,
  pipeline,
  onSuccess,
  initialInputs,
}: TriggerWorkflowModalProps) => {
  const { isMobile } = useIsMobile()
  const [paramValues, setParamValues] = useState<Record<string, any>>({})
  const isSubmittingRef = useRef(false)

  const { data: parameters = [], isLoading: fetchingParams, error: fetchError } = useWorkflowParameters(
    pipeline.provider_id,
    opened ? pipeline.id : ''
  )
  const triggerMutation = useTriggerWorkflow()

  const error = fetchError instanceof Error ? fetchError.message : fetchError ? String(fetchError) : null

  const isPreparingRerun = opened && initialInputs === undefined && parameters.length === 0

  useEffect(() => {
    if (!opened) {
      setParamValues({})
      
return
    }

    if (parameters.length > 0) {
      initializeParamValues(parameters, initialInputs)
    }
  }, [opened, parameters, initialInputs])

  const initializeParamValues = (params: WorkflowParameter[], inputs?: Record<string, any>) => {
    const initialValues: Record<string, any> = {}
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

    setParamValues(initialValues)
  }


  const handleTrigger = async () => {
    if (isSubmittingRef.current || triggerMutation.isPending) {
return
}
    isSubmittingRef.current = true

    const requiredParams = parameters.filter((p) => p.required)

    for (const param of requiredParams) {
      const value = paramValues[param.name]

      if (value === undefined || value === null || (typeof value === 'string' && value.trim() === '')) {
        notifications.show({
          title: 'Error',
          message: `Parameter "${param.label || param.name}" is required`,
          color: 'red',
        })
        isSubmittingRef.current = false
        
return
      }
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
          let shouldOpenLogs = false

          try {
            const parsed = JSON.parse(result)


            runNumber = parsed.run_number || parsed.build_number || parsed.number || 0
            if (runNumber > 0) {
              shouldOpenLogs = true
            }
          } catch { /* parse error expected for non-JSON responses */ }

          onClose()

          if (onSuccess && shouldOpenLogs) {
            onSuccess(pipeline.id, runNumber)
          } else if (runNumber === 0) {
            notifications.show({
              title: 'Note',
              message: 'Workflow triggered but run number not available yet. Check the pipeline list for the new run.',
              color: 'yellow',
              autoClose: 5000,
            })
          }
        },
        onError: (error: Error) => {
          isSubmittingRef.current = false
          displayErrorNotification(error, 'Failed to Trigger Workflow')
        },
      }
    )
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
            checked={value ?? false}
            onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.checked })}
            disabled={isDisabled}
          />
        )

      case 'choice': {
        const options = param.options ? [...new Set(param.options)] : []
        const validValue = value && options.includes(value) ? value : null

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
            value={value ?? null}
            onChange={(val) => setParamValues({ ...paramValues, [param.name]: val })}
            required={param.required}
            disabled={isDisabled}
          />
        )

      case 'string':
      default:
        return (
          <TextInput
            key={param.name}
            label={label}
            description={param.description || undefined}
            placeholder={`Enter ${label.toLowerCase()}`}
            value={value || ''}
            onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.value })}
            required={param.required}
            disabled={isDisabled}
          />
        )
    }
  }

  const footer = (
    <Button
      onClick={handleTrigger}
      variant="light"
      color="blue"
      fullWidth
      size={isMobile ? 'sm' : 'md'}
      disabled={isPreparingRerun || fetchingParams || !!error || triggerMutation.isPending}
    >
      {triggerMutation.isPending ? 'Triggering...' : 'Trigger Workflow'}
    </Button>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={initialInputs ? `Rerun: ${pipeline.name}` : `Trigger: ${pipeline.name}`}
      loading={triggerMutation.isPending}
      footer={footer}
      contentPadding={false}
    >
      <Stack gap={isMobile ? 'xs' : 'md'} p="md">
        {isPreparingRerun || fetchingParams ? (
          <Stack align="center" gap="sm" py="md">
            <Loader size="sm" />
            <Text size="sm" c="dimmed">
              {isPreparingRerun ? 'Preparing rerun...' : 'Loading workflow parameters...'}
            </Text>
          </Stack>
        ) : error ? (
          <Alert icon={<IconAlertCircle size={16} />} title="Error Loading Parameters" color="red">
            {error}
          </Alert>
        ) : parameters.length > 0 ? (
          <Stack gap={isMobile ? 'xs' : 'sm'}>
            {parameters.map((param) => renderParameterInput(param))}
          </Stack>
        ) : (
          <Text size="sm" c="dimmed" ta="center" py="sm">
            No parameters required for this workflow
          </Text>
        )}
      </Stack>
    </StandardModal>
  )
}
