import type { FeatureAvailability, PermissionStatus, PluginMetadata, ProviderConfig } from '../../../types'

export type Step = 'credentials' | 'pipelines'

export interface FormState {
  step: Step

  selectedPlugin: PluginMetadata | null
  providerName: string

  configValues: Record<string, string>
  dynamicOptions: Record<string, string[]>

  selectedOrganization: string
  selectedPipelines: Set<string>
  repositorySearch: string

  submitting: boolean
  error: string | null
  fieldErrors: Record<string, string>
}

export interface PermissionState {
  modalOpen: boolean
  status: PermissionStatus | null
  features: FeatureAvailability[]
  checking: boolean
  error: string | null
}

export type FormAction =
  | { type: 'SET_STEP'; step: Step }
  | { type: 'SELECT_PLUGIN'; plugin: PluginMetadata; isEditMode: boolean }
  | { type: 'CLEAR_PLUGIN' }
  | { type: 'SET_PROVIDER_NAME'; name: string }
  | { type: 'UPDATE_CONFIG'; key: string; value: string }
  | { type: 'SET_CONFIG_VALUES'; values: Record<string, string> }
  | { type: 'SET_DYNAMIC_OPTIONS'; options: Record<string, string[]> }
  | { type: 'SET_ORGANIZATION'; organization: string }
  | { type: 'TOGGLE_PIPELINE'; pipelineId: string }
  | { type: 'SET_SELECTED_PIPELINES'; pipelineIds: Set<string> }
  | { type: 'SELECT_ALL_PIPELINES'; pipelineIds: string[] }
  | { type: 'CLEAR_SELECTED_PIPELINES' }
  | { type: 'SET_REPOSITORY_SEARCH'; search: string }
  | { type: 'SET_SUBMITTING'; submitting: boolean }
  | { type: 'SET_ERROR'; error: string | null }
  | { type: 'SET_FIELD_ERROR'; key: string; error: string }
  | { type: 'CLEAR_FIELD_ERROR'; key: string }
  | { type: 'SET_FIELD_ERRORS'; errors: Record<string, string> }
  | { type: 'RESET' }
  | { type: 'INIT_EDIT_MODE'; plugin: PluginMetadata; provider: ProviderConfig & { id: number } }

export interface AddProviderModalProps {
  opened: boolean
  onClose: () => void
  onAdd?: (config: ProviderConfig) => Promise<void>
  onUpdate?: (id: number, config: ProviderConfig) => Promise<void>
  editMode?: boolean
  existingProvider?: ProviderConfig & { id: number }
}

export const initialFormState: FormState = {
  step: 'credentials',
  selectedPlugin: null,
  providerName: '',
  configValues: {},
  dynamicOptions: {},
  selectedOrganization: '',
  selectedPipelines: new Set(),
  repositorySearch: '',
  submitting: false,
  error: null,
  fieldErrors: {},
}

export const initialPermissionState: PermissionState = {
  modalOpen: false,
  status: null,
  features: [],
  checking: false,
  error: null,
}
