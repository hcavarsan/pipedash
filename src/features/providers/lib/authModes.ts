import type { AuthMode, AuthOption, ConfigField, PluginMetadata } from '../../../types/plugins'

const NO_OPTIONS: AuthOption[] = []

export function authOptions(plugin: PluginMetadata | null): AuthOption[] {
  return plugin?.config_schema.auth_options ?? NO_OPTIONS
}

export function defaultAuthMode(plugin: PluginMetadata | null): AuthMode {
  return authOptions(plugin)[0]?.mode ?? 'token'
}

export function fieldInMode(field: ConfigField, mode: AuthMode): boolean {
  return !field.auth_modes?.length || field.auth_modes.includes(mode)
}
