import type { Organization, PaginatedAvailablePipelines } from '../../types/pipelines'
import type { PermissionCheckResult, PluginMetadata, TableSchema } from '../../types/plugins'
import type { ProviderDraftResult } from '../../types/provider_draft'
import type {
  ProviderConfig,
  ProviderImportResult,
  ProviderSummary,
  SignInStart,
  SignInStatus,
} from '../../types/providers'
import { isRemoteClient } from '../connection'
import type { HttpClient } from '../http'
import { tauriPlatformService } from '../tauri/platform'
import type { ProvidersService } from '../types'

export function createProvidersApi(client: HttpClient): ProvidersService {
  const openSignIn = async (path: string, body?: unknown): Promise<SignInStart> => {
    if (isRemoteClient()) {
      const start = await client.post<SignInStart>(path, body)

      await tauriPlatformService.openUrl(start.authorize_url)

      return start
    }

    const tab = window.open('', '_blank')

    if (!tab) {
      throw new Error('Your browser blocked the sign-in tab. Allow pop-ups for this site, then choose Open again.')
    }

    try {
      const start = await client.post<SignInStart>(path, body)

      tab.opener = null
      tab.location.href = start.authorize_url

      return start
    } catch (error) {
      tab.close()
      throw error
    }
  }

  return {
    listProviders: async () => {
      const result = await client.get<ProviderSummary[]>('/providers')

      return result || []
    },

    getProvider: async (id) => {
      const config = await client.get<ProviderConfig>(`/providers/${id}`)

      return { ...config, id }
    },

    updateProviderRefreshInterval: async (id, refreshInterval) => {
      await client.requestVoid('PUT', `/providers/${id}/refresh-interval`, {
        refresh_interval: refreshInterval,
      })
    },

    removeProvider: async (id) => {
      await client.requestVoid('DELETE', `/providers/${id}`)
    },

    exportProviders: async () => client.get<string>('/providers/export'),

    importProviders: async (content) => client.post<ProviderImportResult>('/providers/import', { content }),

    fetchProviderOrganizations: async (providerType, config) =>
      client.post<Organization[]>('/providers/organizations', {
        provider_type: providerType,
        config,
      }),

    previewProviderPipelines: async (providerType, config, org, search, page, pageSize) =>
      client.post<PaginatedAvailablePipelines>('/providers/preview', {
        provider_type: providerType,
        config,
        org: org ?? null,
        search: search ?? null,
        page,
        page_size: pageSize,
      }),

    getProviderFieldOptions: async (providerType, fieldKey, config) =>
      client.post<string[]>('/providers/field-options', {
        provider_type: providerType,
        field_key: fieldKey,
        config,
      }),

    getProviderTableSchema: async (providerId) => client.get<TableSchema>(`/providers/${providerId}/table-schema`),

    startSignIn: async (providerType, config) => openSignIn('/sign-in', { provider_type: providerType, config }),

    startProviderSignIn: async (id) => openSignIn(`/providers/${id}/sign-in`),

    reconnectProvider: async (id, flowId) => {
      await client.requestVoid('POST', `/providers/${id}/sign-in/complete`, { flow_id: flowId })
    },

    signInStatus: async (id) => client.get<SignInStatus>(`/sign-in/${encodeURIComponent(id)}`),

    completeSignIn: async (id, redirectUrl) =>
      client.post<SignInStatus>(`/sign-in/${encodeURIComponent(id)}/complete`, { redirect_url: redirectUrl }),

    cancelSignIn: async (id) => {
      await client.requestVoid('DELETE', `/sign-in/${encodeURIComponent(id)}`)
    },

    checkProviderPermissions: async (providerType, config) =>
      client.post<PermissionCheckResult>('/providers/permissions/check', {
        provider_type: providerType,
        config,
      }),

    getAvailablePlugins: async () => client.get<PluginMetadata[]>('/plugins'),

    validateProviderDraft: async (draft) => client.post<ProviderDraftResult>('/providers/draft/validate', draft),

    saveProviderDraft: async (draft) => client.post<number>('/providers/draft', draft),

    getMatchingPipelineIds: async (providerType, config, organization, search) =>
      client.post<string[]>('/providers/preview/ids', {
        provider_type: providerType,
        config,
        organization: organization ?? null,
        search: search ?? null,
      }),
  }
}
