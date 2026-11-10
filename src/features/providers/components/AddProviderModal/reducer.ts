import type { ConfigField } from '../../../../types/plugins'
import type { ProviderConfig } from '../../../../types/providers'
import { authOptions, defaultAuthMode, fieldInMode } from '../../lib/authModes'
import { AUTH_MODE_KEY, SIGN_IN_KEY, TOKEN_KEY } from '../../lib/credentials'

import { type FieldRequest, type FormAction, type FormState, initialFormState } from './types'

function seedDefaults(config: Record<string, string>, fields: ConfigField[]): void {
  for (const field of fields) {
    if (config[field.key] === undefined && field.default_value !== null && field.default_value !== undefined) {
      config[field.key] = String(field.default_value)
    }
  }
}

export function fieldRequest(state: FormState, key: string): FieldRequest {
  return {
    value: key === 'providerName' ? state.providerName : state.configValues[key],
    providerType: state.selectedPlugin?.provider_type ?? null,
    authMode: state.authMode,
  }
}

function answersCurrentField(state: FormState, key: string, requested: FieldRequest): boolean {
  const current = fieldRequest(state, key)

  return (
    current.value === requested.value &&
    current.providerType === requested.providerType &&
    current.authMode === requested.authMode
  )
}

export function formReducer(state: FormState, action: FormAction): FormState {
  switch (action.type) {
    case 'SET_STEP':
      return { ...state, step: action.step, selectingAll: false }

    case 'SELECT_PLUGIN': {
      const mode = defaultAuthMode(action.plugin)
      const initialConfig: Record<string, string> = {}

      seedDefaults(
        initialConfig,
        action.plugin.config_schema.fields.filter((field) => fieldInMode(field, mode))
      )

      if (authOptions(action.plugin).length > 0) {
        initialConfig[AUTH_MODE_KEY] = mode
      }

      return {
        ...state,
        selectedPlugin: action.plugin,
        providerName: action.isEditMode ? state.providerName : action.plugin.name,
        authMode: mode,
        configValues: initialConfig,
        selectedOrganization: '',
        selectedPipelines: new Set(),
        repositorySearch: '',
        fieldErrors: {},
      }
    }

    case 'CLEAR_PLUGIN':
      return {
        ...state,
        selectedPlugin: null,
        configValues: {},
        authMode: 'token',
        providerName: '',
        selectedOrganization: '',
        selectedPipelines: new Set(),
        repositorySearch: '',
        fieldErrors: {},
      }

    case 'SET_PROVIDER_NAME':
      return { ...state, providerName: action.name }

    case 'UPDATE_CONFIG':
      return {
        ...state,
        configValues: { ...state.configValues, [action.key]: action.value },
        selectedOrganization: '',
      }

    case 'SET_CONFIG_VALUES':
      return { ...state, configValues: action.values }

    case 'SET_AUTH_MODE': {
      const configValues = { ...state.configValues }
      const fieldErrors = { ...state.fieldErrors }
      const fields = state.selectedPlugin?.config_schema.fields ?? []
      const active = fields.filter((field) => fieldInMode(field, action.mode))
      const activeKeys = new Set(active.map((field) => field.key))
      const dropped = [
        TOKEN_KEY,
        SIGN_IN_KEY,
        ...fields.map((field) => field.key).filter((key) => !activeKeys.has(key)),
      ]

      for (const key of dropped) {
        delete configValues[key]
        delete fieldErrors[key]
      }
      seedDefaults(configValues, active)
      configValues[AUTH_MODE_KEY] = action.mode

      return { ...state, authMode: action.mode, configValues, fieldErrors }
    }

    case 'SET_SIGN_IN': {
      const configValues = { ...state.configValues }
      const fieldErrors = { ...state.fieldErrors }

      if (action.reference === null) {
        delete configValues[SIGN_IN_KEY]
      } else {
        configValues[SIGN_IN_KEY] = action.reference
      }
      delete fieldErrors[SIGN_IN_KEY]

      return { ...state, configValues, fieldErrors }
    }

    case 'SET_ORGANIZATION':
      return {
        ...state,
        selectedOrganization: action.organization,
        repositorySearch: '',
        selectingAll: false,
      }

    case 'TOGGLE_PIPELINE': {
      const newSet = new Set(state.selectedPipelines)

      if (newSet.has(action.pipelineId)) {
        newSet.delete(action.pipelineId)
      } else {
        newSet.add(action.pipelineId)
      }

      return { ...state, selectedPipelines: newSet }
    }

    case 'SELECT_ALL_PIPELINES':
      return { ...state, selectedPipelines: new Set([...state.selectedPipelines, ...action.pipelineIds]) }

    case 'DESELECT_PIPELINES': {
      const selectedPipelines = new Set(state.selectedPipelines)

      for (const id of action.pipelineIds) {
        selectedPipelines.delete(id)
      }

      return { ...state, selectedPipelines }
    }

    case 'CLEAR_PIPELINES':
      return { ...state, selectedPipelines: new Set(), repositorySearch: '' }

    case 'START_SELECT_ALL':
      return { ...state, selectingAll: true, selectAllRun: action.run }

    case 'FINISH_SELECT_ALL':
      if (!state.selectingAll || action.run !== state.selectAllRun) {
        return state
      }

      return {
        ...state,
        selectingAll: false,
        selectedPipelines: action.pipelineIds
          ? new Set([...state.selectedPipelines, ...action.pipelineIds])
          : state.selectedPipelines,
      }

    case 'SET_REPOSITORY_SEARCH':
      return { ...state, repositorySearch: action.search, selectingAll: false }

    case 'SET_ERROR':
      return { ...state, error: action.error }

    case 'SET_FIELD_ERROR':
      if (!answersCurrentField(state, action.key, action.requested)) {
        return state
      }

      return {
        ...state,
        fieldErrors: { ...state.fieldErrors, [action.key]: action.error },
      }

    case 'CLEAR_FIELD_ERROR': {
      if (!answersCurrentField(state, action.key, action.requested)) {
        return state
      }

      const newErrors = { ...state.fieldErrors }

      delete newErrors[action.key]

      return { ...state, fieldErrors: newErrors }
    }

    case 'SET_FIELD_ERRORS':
      return { ...state, fieldErrors: action.errors }

    case 'RESET':
      return initialFormState

    case 'INIT_EDIT_MODE': {
      const initialConfig = { ...action.provider.config }

      if (action.provider.token) {
        initialConfig[TOKEN_KEY] = action.provider.token
      }

      return {
        ...state,
        selectedPlugin: action.plugin,
        providerName: action.provider.config.display_name || action.provider.name,
        authMode:
          authOptions(action.plugin).find((option) => option.mode === initialConfig[AUTH_MODE_KEY])?.mode ??
          defaultAuthMode(action.plugin),
        configValues: initialConfig,
        selectedPipelines: new Set(
          (initialConfig.selected_items ?? '')
            .split(',')
            .map((id) => id.trim())
            .filter(Boolean)
        ),
      }
    }

    default:
      return state
  }
}

const snapshot = (state: FormState): string =>
  JSON.stringify({
    plugin: state.selectedPlugin?.provider_type ?? null,
    name: state.providerName,
    config: Object.entries(state.configValues)
      .filter(([key, value]) => key !== 'selected_items' && value !== '')
      .sort(([a], [b]) => a.localeCompare(b)),
    pipelines: [...state.selectedPipelines].sort(),
  })

export function isFormDirty(state: FormState, existingProvider?: ProviderConfig & { id: number }): boolean {
  if (!state.selectedPlugin) {
    return false
  }

  const pristine = existingProvider
    ? formReducer(initialFormState, {
        type: 'INIT_EDIT_MODE',
        plugin: state.selectedPlugin,
        provider: existingProvider,
      })
    : formReducer(initialFormState, { type: 'SELECT_PLUGIN', plugin: state.selectedPlugin, isEditMode: false })

  return snapshot(state) !== snapshot(pristine)
}
