import { type FormAction, type FormState, initialFormState } from './types'

export function formReducer(state: FormState, action: FormAction): FormState {
  switch (action.type) {
    case 'SET_STEP':
      return { ...state, step: action.step }

    case 'SELECT_PLUGIN': {
      const initialConfig: Record<string, string> = {}

      action.plugin.config_schema.fields.forEach((field) => {
        if (field.default_value) {
          const defaultVal = typeof field.default_value === 'string'
            ? field.default_value
            : String(field.default_value)

          initialConfig[field.key] = defaultVal
        }
      })

      return {
        ...state,
        selectedPlugin: action.plugin,
        providerName: action.isEditMode ? state.providerName : action.plugin.name,
        configValues: initialConfig,
        dynamicOptions: {},
      }
    }

    case 'CLEAR_PLUGIN':
      return {
        ...state,
        selectedPlugin: null,
        configValues: {},
        providerName: '',
        dynamicOptions: {},
      }

    case 'SET_PROVIDER_NAME':
      return {
        ...state,
        providerName: action.name,
        fieldErrors: { ...state.fieldErrors, providerName: '' },
      }

    case 'UPDATE_CONFIG': {
      const newFieldErrors = { ...state.fieldErrors }

      delete newFieldErrors[action.key]

      return {
        ...state,
        configValues: { ...state.configValues, [action.key]: action.value },
        fieldErrors: newFieldErrors,
      }
    }

    case 'SET_CONFIG_VALUES':
      return { ...state, configValues: action.values }

    case 'SET_DYNAMIC_OPTIONS':
      return { ...state, dynamicOptions: { ...state.dynamicOptions, ...action.options } }

    case 'SET_ORGANIZATION':
      return {
        ...state,
        selectedOrganization: action.organization,
        repositorySearch: '',
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

    case 'SET_SELECTED_PIPELINES':
      return { ...state, selectedPipelines: action.pipelineIds }

    case 'SELECT_ALL_PIPELINES':
      return { ...state, selectedPipelines: new Set(action.pipelineIds) }

    case 'CLEAR_SELECTED_PIPELINES':
      return { ...state, selectedPipelines: new Set() }

    case 'SET_REPOSITORY_SEARCH':
      return { ...state, repositorySearch: action.search }

    case 'SET_SUBMITTING':
      return { ...state, submitting: action.submitting }

    case 'SET_ERROR':
      return { ...state, error: action.error }

    case 'SET_FIELD_ERROR':
      return {
        ...state,
        fieldErrors: { ...state.fieldErrors, [action.key]: action.error },
      }

    case 'CLEAR_FIELD_ERROR': {
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
        initialConfig.token = action.provider.token
      }

      return {
        ...state,
        selectedPlugin: action.plugin,
        providerName: action.provider.name,
        configValues: initialConfig,
      }
    }

    default:
      return state
  }
}
