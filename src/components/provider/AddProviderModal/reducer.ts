import { type FormAction, type FormState, initialFormState } from './types'

export function formReducer(state: FormState, action: FormAction): FormState {
  switch (action.type) {
    case 'SET_STEP':
      return { ...state, step: action.step }

    case 'SELECT_PLUGIN': {
      const initialConfig: Record<string, string> = {}

      action.plugin.config_schema.fields.forEach((field) => {
        if (field.default_value !== null && field.default_value !== undefined) {
          initialConfig[field.key] = String(field.default_value)
        }
      })

      return {
        ...state,
        selectedPlugin: action.plugin,
        providerName: action.isEditMode ? state.providerName : action.plugin.name,
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
        providerName: '',
        selectedOrganization: '',
        selectedPipelines: new Set(),
        repositorySearch: '',
        fieldErrors: {},
      }

    case 'SET_PROVIDER_NAME': {
      const { providerName: _cleared, ...fieldErrors } = state.fieldErrors

      return { ...state, providerName: action.name, fieldErrors }
    }

    case 'UPDATE_CONFIG': {
      const newFieldErrors = { ...state.fieldErrors }

      delete newFieldErrors[action.key]
      const scopeChanged =
        (action.key === 'kubeconfig' || action.key === 'context' || action.key === 'namespaces') &&
        state.configValues[action.key] !== action.value
      const configValues = { ...state.configValues, [action.key]: action.value }

      if (scopeChanged) {
        delete configValues.selected_items
        if (action.key === 'kubeconfig') {
          configValues.context = ''
        }
      }

      return {
        ...state,
        configValues,
        selectedPipelines: scopeChanged ? new Set() : state.selectedPipelines,
        repositorySearch: scopeChanged ? '' : state.repositorySearch,
        selectedOrganization: '',
        fieldErrors: newFieldErrors,
      }
    }

    case 'SET_CONFIG_VALUES':
      return { ...state, configValues: action.values }

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

    case 'SELECT_ALL_PIPELINES':
      return { ...state, selectedPipelines: new Set([...state.selectedPipelines, ...action.pipelineIds]) }

    case 'DESELECT_PIPELINES': {
      const selectedPipelines = new Set(state.selectedPipelines)

      for (const id of action.pipelineIds) {
        selectedPipelines.delete(id)
      }

      return { ...state, selectedPipelines }
    }

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
