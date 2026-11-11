import type { ProviderDraft } from '../../../../types/provider_draft'

import type { FormState } from './types'

type DraftSource = Pick<FormState, 'selectedPlugin' | 'providerName' | 'authMode' | 'configValues'>

export function buildProviderDraft(
  { selectedPlugin, providerName, authMode, configValues }: DraftSource,
  providerId: number | null,
  selectedPipelineIds: string[] | null
): ProviderDraft | null {
  if (!selectedPlugin) {
    return null
  }

  return {
    provider_type: selectedPlugin.provider_type,
    name: providerName,
    auth_mode: authMode,
    values: configValues,
    selected_pipeline_ids: selectedPipelineIds,
    provider_id: providerId,
  }
}
