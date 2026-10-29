import type { ProviderSummary } from '../types/domain'

export function patchProviderList(
  list: ProviderSummary[] | undefined,
  summary: ProviderSummary
): ProviderSummary[] | undefined {
  if (!list?.some((provider) => provider.id === summary.id)) {
    return list
  }

  return list.map((provider) => (provider.id === summary.id ? summary : provider))
}
