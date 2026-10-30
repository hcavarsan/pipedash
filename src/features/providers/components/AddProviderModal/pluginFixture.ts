import type { ConfigField, PluginMetadata } from '../../../../types/domain'

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
    name: 'GitHub',
    provider_type: 'github',
    version: '1.0.0',
    description: 'GitHub Actions',
    author: null,
    icon: null,
    config_schema: { fields: [configFieldFixture()] },
    table_schema: { tables: [] },
    required_permissions: [{ name: 'repo', description: 'Read repositories', required: true }],
    features: [],
    capabilities: {
      pipelines: true,
      pipeline_runs: true,
      trigger: true,
      cancel: true,
      agents: false,
      artifacts: false,
      queues: false,
      custom_tables: false,
      retains_run_history: true,
      logs: false,
      sign_in: false,
    },
    ...overrides,
  }
}
