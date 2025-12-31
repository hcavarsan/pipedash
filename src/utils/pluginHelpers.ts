import type { PluginMetadata } from '../types'

export function getPluginByType(
  plugins: PluginMetadata[] | undefined,
  providerType: string
): PluginMetadata | undefined {
  return plugins?.find((p) => p.provider_type === providerType)
}

export function getPluginDisplayName(
  plugins: PluginMetadata[] | undefined,
  providerType: string
): string {
  const plugin = getPluginByType(plugins, providerType)

  return plugin?.name || providerType
}
