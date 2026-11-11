import type { PluginPresentation } from './plugin_presentation'

interface PluginCapabilities {
  trigger: boolean
  cancel: boolean
  force_cancel: boolean
  retains_run_history: boolean
  logs: boolean
  sign_in: boolean
  uses_http_proxy: boolean
  async_trigger: boolean
}

type ConfigFieldType = 'Text' | 'Password' | 'TextArea' | 'Boolean' | 'Select' | 'MultiSelect' | 'Number'

export type ConfigSection = 'authentication' | 'connection' | 'settings'

export interface ConfigField {
  key: string
  label: string
  description: string | null
  field_type: ConfigFieldType
  required: boolean
  default_value: string | number | boolean | null
  options: string[] | null
  validation_regex: string | null
  validation_message: string | null
  section: ConfigSection
  auth_modes?: AuthMode[]
}

export type AuthMode = 'none' | 'token' | 'sign_in' | 'kubeconfig'

export interface AuthOption {
  mode: AuthMode
  label: string
  description: string
}

interface ConfigSchema {
  fields: ConfigField[]
  auth_options?: AuthOption[]
}

export type ColumnDataType =
  | 'String'
  | 'Number'
  | 'DateTime'
  | 'Duration'
  | 'Status'
  | 'Badge'
  | 'Url'
  | 'Json'
  | 'Boolean'
  | { Custom: string }

export type CellRenderer =
  | 'Text'
  | 'Badge'
  | 'DateTime'
  | 'Duration'
  | 'StatusBadge'
  | 'Commit'
  | 'Avatar'
  | 'TruncatedText'
  | 'Link'
  | 'JsonViewer'
  | { Custom: string }

export type ColumnVisibility =
  | 'Always'
  | 'WhenPresent'
  | { WhenCapability: string }
  | { Conditional: { field: string; equals: unknown } }

export interface ColumnDefinition {
  id: string
  label: string
  description: string | null
  field_path: string
  data_type: ColumnDataType
  renderer: CellRenderer
  visibility: ColumnVisibility
  default_visible: boolean
  width: number | null
  sort: 'none' | 'value' | 'history_order'
  filterable: boolean
}

export interface TableDefinition {
  id: string
  name: string
  description: string | null
  columns: ColumnDefinition[]
}

export interface TableSchema {
  tables: TableDefinition[]
}

export interface RunHistoryPolicy {
  bounded: boolean
  order: 'run_number' | 'started_at'
  tracks_overdue: boolean
}

export interface PluginMetadata {
  name: string
  provider_type: string
  version: string
  description: string
  author: string | null
  icon: string | null
  config_schema: ConfigSchema
  table_schema: TableSchema
  capabilities: PluginCapabilities
  required_permissions: Permission[]
  features: Feature[]
  run_history: RunHistoryPolicy
  token_prefixes: string[]
  presentation: PluginPresentation
}

export interface Permission {
  name: string
  description: string
  required: boolean
}

export interface PermissionCheck {
  permission: Permission
  granted: boolean
}

export interface PermissionStatus {
  permissions: PermissionCheck[]
  all_granted: boolean
  checked_at: string
  metadata?: Record<string, string>
  credential_label: string | null
}

export interface Feature {
  id: string
  name: string
  description: string
  required_permissions: string[]
}

export interface FeatureAvailability {
  feature: Feature
  available: boolean
  missing_permissions: string[]
}

export interface PermissionCheckResult {
  permission_status: PermissionStatus | null
  features: FeatureAvailability[]
}
