import type { AuthMode, PluginMetadata } from '../../../../types/plugins'
import type { ProviderDraft } from '../../../../types/provider_draft'
import type { ProviderConfig } from '../../../../types/providers'

export type Step = 'provider' | 'credentials' | 'pipelines'

export const STEP_ORDER: readonly Step[] = ['provider', 'credentials', 'pipelines']

export interface SubmitError {
  message: string
  draft: ProviderDraft | null
}

export interface FieldRequest {
  value: string | undefined
  providerType: string | null
  authMode: AuthMode
}

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
  selectAllRun: number

  error: SubmitError | null
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
  | { type: 'CLEAR_PIPELINES' }
  | { type: 'START_SELECT_ALL'; run: number }
  | { type: 'FINISH_SELECT_ALL'; run: number; pipelineIds: string[] | null }
  | { type: 'SET_REPOSITORY_SEARCH'; search: string }
  | { type: 'SET_ERROR'; error: SubmitError | null }
  | { type: 'SET_FIELD_ERROR'; key: string; error: string; requested: FieldRequest }
  | { type: 'CLEAR_FIELD_ERROR'; key: string; requested: FieldRequest }
  | { type: 'SET_FIELD_ERRORS'; errors: Record<string, string> }
  | { type: 'RESET' }
  | { type: 'INIT_EDIT_MODE'; plugin: PluginMetadata; provider: ProviderConfig & { id: number } }

export type AddProviderModalProps = {
  opened: boolean
  onClose: () => void
} & (
  | {
      editMode?: false
      existingProvider?: never
    }
  | {
      editMode: true
      existingProvider: ProviderConfig & { id: number }
    }
)

export type ProviderSubmitIntent = 'continue' | 'save-credentials' | 'save-selection'

export interface ProviderFormActionState {
  error: string | null
  fieldErrors: Record<string, string>
}

export const initialActionState: ProviderFormActionState = { error: null, fieldErrors: {} }

export const initialFormState: FormState = {
  step: 'provider',
  selectedPlugin: null,
  providerName: '',
  authMode: 'token',
  configValues: {},
  selectedOrganization: '',
  selectedPipelines: new Set(),
  repositorySearch: '',
  selectingAll: false,
  selectAllRun: 0,
  error: null,
  fieldErrors: {},
}
