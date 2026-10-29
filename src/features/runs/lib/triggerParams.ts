import type { Pipeline, WorkflowParameter } from '../../../types/domain'
import { isRecord } from '../../../utils/isRecord'

export type ParamValues = Record<string, unknown>

export const triggerModalTitle = (
  pipeline: Pick<Pipeline, 'name'>,
  mobileShell: boolean,
  isFlux: boolean,
  isRerun: boolean
): string => {
  if (mobileShell) {
    return isFlux ? 'Reconcile' : isRerun ? 'Rerun workflow' : 'Run workflow'
  }

  return isFlux ? `Reconcile: ${pipeline.name}` : isRerun ? `Rerun: ${pipeline.name}` : `Trigger: ${pipeline.name}`
}

export const buildInitialParamValues = (params: WorkflowParameter[], inputs?: ParamValues): ParamValues => {
  const initialValues: ParamValues = {}
  const isReplay = inputs !== undefined && Object.keys(inputs).length > 0

  for (const param of params) {
    const replayed = isReplay ? inputs[param.name] : undefined

    if (replayed !== undefined) {
      initialValues[param.name] = replayed
    } else if (param.default !== undefined && param.default !== null) {
      if ((param.type !== 'string' && param.type !== 'choice') || String(param.default).trim() !== '') {
        initialValues[param.name] = param.default
      }
    } else if (param.type === 'boolean') {
      initialValues[param.name] = false
    }
  }

  return initialValues
}

export const comparable = (value: unknown): string => (value === undefined || value === null ? '' : String(value))

export const paramError = (param: WorkflowParameter, value: unknown): string | null => {
  const missing = value === undefined || value === null || (typeof value === 'string' && value.trim() === '')

  return param.required && missing ? `${param.label || param.name} is required` : null
}

export const validateParams = (params: WorkflowParameter[], values: ParamValues): Record<string, string> => {
  const errors: Record<string, string> = {}

  for (const param of params) {
    const error = paramError(param, values[param.name])

    if (error) {
      errors[param.name] = error
    }
  }

  return errors
}

export const parseRunNumber = (result: string): number => {
  try {
    const parsed: unknown = JSON.parse(result)

    if (isRecord(parsed)) {
      for (const key of ['run_number', 'build_number', 'number']) {
        const value = parsed[key]

        if (typeof value === 'number' && value > 0) {
          return value
        }
      }
    }
  } catch {
    return 0
  }

  return 0
}
