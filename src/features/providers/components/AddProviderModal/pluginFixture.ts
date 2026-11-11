import { pluginPresentationFixture } from '../../../../testing/presentation'
import type { ConfigField, PluginMetadata } from '../../../../types/plugins'

export function configFieldFixture(overrides: Partial<ConfigField> = {}): ConfigField {
  return {
    key: 'token',
    label: 'Token',
    description: null,
    field_type: 'Password',
    required: true,
    default_value: null,
    options: null,
    validation_regex: null,
    validation_message: null,
    section: 'authentication',
    ...overrides,
  }
}

export function pluginFixture(overrides: Partial<PluginMetadata> = {}): PluginMetadata {
  return {
    name: 'Stub',
    provider_type: 'stub',
    version: '1.0.0',
    description: 'Stub provider',
    author: null,
    icon: null,
    config_schema: { fields: [configFieldFixture()] },
    table_schema: { tables: [] },
    required_permissions: [{ name: 'repo', description: 'Read repositories', required: true }],
    features: [],
    run_history: { bounded: false, order: 'run_number', tracks_overdue: true },
    token_prefixes: [],
    capabilities: {
      trigger: true,
      cancel: true,
      force_cancel: false,
      retains_run_history: true,
      logs: false,
      sign_in: false,
      uses_http_proxy: false,
      async_trigger: false,
    },
    presentation: pluginPresentationFixture(),
    ...overrides,
  }
}
