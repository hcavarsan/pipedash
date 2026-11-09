import type { AuthMode } from './plugins'

export interface ProviderDraft {
  provider_type: string
  name: string
  auth_mode: AuthMode
  values: Record<string, string>
  selected_pipeline_ids: string[] | null
  provider_id: number | null
}

export interface ProviderDraftResult {
  errors: Record<string, string>
  active_field_keys: string[]
  missing_field_labels: string[]
  credentials_complete: boolean
  selection_required: boolean
  selection_needs_refresh: boolean
}
