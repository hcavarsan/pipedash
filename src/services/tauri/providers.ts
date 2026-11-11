import { invoke } from '@tauri-apps/api/core'

import { logger } from '../../lib/logger'
import { toPipedashError } from '../../types/errors'
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
import { invokeWithTimeout } from '../invoke'
import type { ProvidersService } from '../types'

async function normalized<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (error) {
    throw toPipedashError(error)
  }
}

export const providersTauriService: ProvidersService = {
  listProviders: async () => {
    logger.debug('tauriService', 'Invoking list_providers...')

    try {
      const result = await invoke<ProviderSummary[]>('list_providers')

      logger.debug('tauriService', 'list_providers returned', { count: result?.length ?? 0, unit: 'items' })

      return result
    } catch (err) {
      logger.error('tauriService', 'list_providers error', err)
      throw toPipedashError(err)
    }
  },

  getProvider: async (id) => {
    const config = await normalized<ProviderConfig>('get_provider', { id })

    return { ...config, id }
  },

  updateProviderRefreshInterval: async (id, refreshInterval) =>
    normalized<void>('update_provider_refresh_interval', { id, refreshInterval }),

  removeProvider: async (id) => normalized<void>('remove_provider', { id }),

  exportProviders: async () => normalized<string>('export_providers'),

  importProviders: async (content) => normalized<ProviderImportResult>('import_providers', { content }),

  fetchProviderOrganizations: async (providerType, config) =>
    invokeWithTimeout<Organization[]>('fetch_provider_organizations', { providerType, config }),

  previewProviderPipelines: async (providerType, config, org, search, page, pageSize) =>
    invokeWithTimeout<PaginatedAvailablePipelines>('preview_provider_pipelines', {
      providerType,
      config,
      org: org ?? null,
      search: search ?? null,
      page,
      pageSize,
    }),

  getProviderFieldOptions: async (providerType, fieldKey, config) =>
    normalized<string[]>('get_provider_field_options', { providerType, fieldKey, config }),

  getProviderTableSchema: async (providerId) => normalized<TableSchema>('get_provider_table_schema', { providerId }),

  startSignIn: async (providerType, config) =>
    invokeWithTimeout<SignInStart>('start_sign_in', { providerType, config }),

  startProviderSignIn: async (id) => invokeWithTimeout<SignInStart>('start_provider_sign_in', { id }),

  reconnectProvider: async (id, flowId) => invokeWithTimeout<void>('reconnect_provider', { id, flowId }),

  signInStatus: async (id) => invokeWithTimeout<SignInStatus>('sign_in_status', { id }),

  completeSignIn: async (id, redirectUrl) => invokeWithTimeout<SignInStatus>('complete_sign_in', { id, redirectUrl }),

  cancelSignIn: async (id) => invokeWithTimeout<void>('cancel_sign_in', { id }),

  checkProviderPermissions: async (providerType, config) =>
    normalized<PermissionCheckResult>('check_provider_permissions', { providerType, config }),

  getAvailablePlugins: async () => invoke<PluginMetadata[]>('get_available_plugins'),

  validateProviderDraft: async (draft) => normalized<ProviderDraftResult>('validate_provider_draft', { draft }),

  saveProviderDraft: async (draft) => invokeWithTimeout<number>('save_provider_draft', { draft }),

  getMatchingPipelineIds: async (providerType, config, organization, search) =>
    invokeWithTimeout<string[]>('get_matching_pipeline_ids', {
      providerType,
      config,
      organization: organization ?? null,
      search: search ?? null,
    }),
}
