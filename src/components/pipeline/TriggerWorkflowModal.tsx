import { useEffect, useState } from 'react'

import { Alert, Box, Button, Loader, NumberInput, Select, Stack, Switch, Text, TextInput } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { tauriService } from '../../services/tauri'
import type { Pipeline, TriggerParams, WorkflowParameter } from '../../types'
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
  const isMobile = useIsMobile()
  const [loading, setLoading] = useState(false)
  const [fetchingParams, setFetchingParams] = useState(false)
  const [parameters, setParameters] = useState<WorkflowParameter[]>([])
  const [paramValues, setParamValues] = useState<Record<string, any>>({})
  const [error, setError] = useState<string | null>(null)
  const [isPreparingRerun, setIsPreparingRerun] = useState(false)

  useEffect(() => {
    if (opened) {
      if (initialInputs === undefined) {
        setIsPreparingRerun(true)
      }
      fetchParameters()
    } else {
      setParamValues({})
      setError(null)
      setIsPreparingRerun(false)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, pipeline.id])

  useEffect(() => {
    if (initialInputs !== undefined) {
      if (isPreparingRerun) {
        setIsPreparingRerun(false)
      }
      if (parameters.length > 0) {
        initializeParamValues(parameters)
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialInputs, parameters])

  const initializeParamValues = (params: WorkflowParameter[]) => {
    const initialValues: Record<string, any> = {}
    const isReplay = initialInputs !== undefined && Object.keys(initialInputs).length > 0

    params.forEach((param) => {
      if (isReplay && initialInputs && initialInputs[param.name] !== undefined) {
        initialValues[param.name] = initialInputs[param.name]
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

  const fetchParameters = async () => {
    setFetchingParams(true)
    setError(null)
    try {
      const params = await tauriService.getWorkflowParameters(pipeline.id)


      setParameters(params)
      initializeParamValues(params)
    } catch (err: any) {
      console.error('Failed to fetch workflow parameters:', err)
      const errorMsg = err?.error || err?.message || 'Failed to load workflow parameters'


      setError(errorMsg)
      setParameters([])
    } finally {
      setFetchingParams(false)
    }
  }

  const handleTrigger = async () => {
    const requiredParams = parameters.filter((p) => p.required)


    for (const param of requiredParams) {
      const value = paramValues[param.name]


      if (value === undefined || value === null || (typeof value === 'string' && value.trim() === '')) {
        notifications.show({
          title: 'Error',
          message: `Parameter "${param.label || param.name}" is required`,
          color: 'red',
        })

return
      }
    }

    setLoading(true)
    try {
      const params: TriggerParams = {
        workflow_id: pipeline.id,
        inputs: Object.keys(paramValues).length > 0 ? paramValues : undefined,
      }

      console.log('[Trigger] Triggering workflow with params:', params)

      const result = await tauriService.triggerPipeline(params)


      console.log('[Trigger] Raw response:', result)

      let runNumber = 0
      let shouldOpenLogs = false

      try {
        const parsed = JSON.parse(result)


        console.log('[Trigger] Parsed response:', parsed)

        runNumber = parsed.run_number || parsed.build_number || parsed.number || 0

        if (runNumber > 0) {
          shouldOpenLogs = true
        }

        console.log('[Trigger] Extracted run number:', runNumber)
      } catch (parseError) {
        console.error('[Trigger] Failed to parse response as JSON:', parseError)
        console.error('[Trigger] Response text:', result)
      }


      onClose()

      if (onSuccess && shouldOpenLogs) {
        console.log('[Trigger] Opening logs modal for run #', runNumber)
        await onSuccess(pipeline.id, runNumber)
      } else if (runNumber === 0) {
        console.warn('[Trigger] No run number returned, cannot open logs modal')
        notifications.show({
          title: 'Note',
          message: 'Workflow triggered but run number not available yet. Check the pipeline list for the new run.',
          color: 'yellow',
          autoClose: 5000,
        })
      }
    } catch (error: any) {
      console.error('Failed to trigger workflow:', error)
      const errorMsg = error?.error || error?.message || 'Failed to trigger workflow'


      notifications.show({
        title: 'Error',
        message: errorMsg,
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const renderParameterInput = (param: WorkflowParameter) => {
    const label = param.label || param.name
    const value = paramValues[param.name]

    switch (param.type) {
      case 'boolean':
        return (
          <Switch
            key={param.name}
            label={label}
            description={param.description || undefined}
            checked={value ?? false}
            onChange={(e) => setParamValues({ ...paramValues, [param.name]: e.currentTarget.checked })}
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
          />
        )
    }
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={initialInputs ? `Rerun: ${pipeline.name}` : `Trigger: ${pipeline.name}`}
      loading={loading}
    >
      <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
          <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, overflow: 'auto' }}>
            {isPreparingRerun || fetchingParams ? (
              <Stack align="center" gap="sm" py="md">
                <Loader size="sm" />
                <Text size="sm" c="dimmed">
                  {isPreparingRerun ? 'Preparing rerun...' : 'Loading workflow parameters...'}
                </Text>
              </Stack>
            ) : error ? (
              <Stack gap="sm">
                <Alert icon={<IconAlertCircle size={16} />} title="Error Loading Parameters" color="red">
                  {error}
                </Alert>
                <Button onClick={fetchParameters} variant="light" color="blue" fullWidth>
                  Retry
                </Button>
              </Stack>
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
        </Box>

        <Box
          style={{
            borderTop: '1px solid var(--mantine-color-default-border)',
            paddingTop: isMobile ? 8 : 12,
            marginTop: 0,
            flexShrink: 0,
          }}
        >
          <Button
            onClick={handleTrigger}
            variant="light"
            color="blue"
            fullWidth
            size={isMobile ? 'sm' : 'md'}
            disabled={isPreparingRerun || fetchingParams || !!error || loading}
          >
            {loading ? 'Triggering...' : 'Trigger Workflow'}
          </Button>
        </Box>
      </Stack>
    </StandardModal>
  )
}
