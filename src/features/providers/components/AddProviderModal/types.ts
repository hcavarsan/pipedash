import type { AuthMode, PluginMetadata, ProviderConfig } from '../../../../types/domain'

export type Step = 'credentials' | 'pipelines'

export interface FormState {
  step: Step

  selectedPlugin: PluginMetadata | null
  providerName: string

  authMode: AuthMode

  configValues: Record<string, string>

  selectedOrganization: string
  selectedPipelines: Set<string>
  repositorySearch: string
  selectingAll: boolean

  error: string | null
  fieldErrors: Record<string, string>
}

export type FormAction =
  | { type: 'SET_STEP'; step: Step }
  | { type: 'SELECT_PLUGIN'; plugin: PluginMetadata; isEditMode: boolean }
  | { type: 'CLEAR_PLUGIN' }
  | { type: 'SET_PROVIDER_NAME'; name: string }
  | { type: 'UPDATE_CONFIG'; key: string; value: string }
  | { type: 'SET_CONFIG_VALUES'; values: Record<string, string> }
  | { type: 'SET_AUTH_MODE'; mode: AuthMode }
  | { type: 'SET_SIGN_IN'; reference: string | null }
  | { type: 'SET_ORGANIZATION'; organization: string }
  | { type: 'TOGGLE_PIPELINE'; pipelineId: string }
  | { type: 'SELECT_ALL_PIPELINES'; pipelineIds: string[] }
  | { type: 'DESELECT_PIPELINES'; pipelineIds: string[] }
  | { type: 'SET_SELECTING_ALL'; selecting: boolean }
  | { type: 'SET_REPOSITORY_SEARCH'; search: string }
  | { type: 'SET_ERROR'; error: string | null }
  | { type: 'SET_FIELD_ERROR'; key: string; error: string }
  | { type: 'CLEAR_FIELD_ERROR'; key: string }
  | { type: 'SET_FIELD_ERRORS'; errors: Record<string, string> }
  | { type: 'RESET' }
  | { type: 'INIT_EDIT_MODE'; plugin: PluginMetadata; provider: ProviderConfig & { id: number } }

export type AddProviderModalProps = {
  opened: boolean
  onClose: () => void
} & (
  | {
      editMode?: false
      onAdd: (config: ProviderConfig) => Promise<void>
      onUpdate?: never
      existingProvider?: never
    }
  | {
      editMode: true
      onUpdate: (id: number, config: ProviderConfig) => Promise<void>
      existingProvider: ProviderConfig & { id: number }
      onAdd?: never
    }
)

export type ProviderSubmitIntent = 'continue' | 'save-credentials' | 'save-selection'

export interface ProviderFormActionState {
  error: string | null
  fieldErrors: Record<string, string>
}

export const initialActionState: ProviderFormActionState = { error: null, fieldErrors: {} }

export const initialFormState: FormState = {
  step: 'credentials',
  selectedPlugin: null,
  providerName: '',
  authMode: 'token',
  configValues: {},
  selectedOrganization: '',
  selectedPipelines: new Set(),
  repositorySearch: '',
  selectingAll: false,
  error: null,
  fieldErrors: {},
}
