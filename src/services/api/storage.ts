import type { SetupApplied, SetupDraftResult } from '../../types/setup'
import type {
  ConfigAnalysisResponse,
  ConfigContentResponse,
  MigrationResult,
  SetupStatus,
  StorageConfigResponse,
  StoragePathsResponse,
} from '../../types/storage'
import type { UnlockVaultResponse, VaultStatusResponse } from '../../types/vault'
import type { HttpClient } from '../http'
import type { StorageService } from '../types'

export function createStorageApi(client: HttpClient): StorageService {
  return {
    checkSetupStatus: async () => client.get<SetupStatus>('/setup/status'),

    prepareSetupDraft: async (draft) => client.post<SetupDraftResult>('/setup/draft', draft),

    applySetupDraft: async (draft) => client.post<SetupApplied>('/setup/apply', draft),

    getStorageConfig: async () => client.get<StorageConfigResponse>('/storage/config'),

    getVaultPasswordStatus: async () =>
      client.get<{ is_set: boolean; env_var_name: string }>('/storage/vault-password-status'),

    getVaultStatus: async () => client.get<VaultStatusResponse>('/vault/status'),

    unlockVault: async (password) => client.post<UnlockVaultResponse>('/vault/unlock', { password }),

    lockVault: async () => client.post<UnlockVaultResponse>('/vault/lock'),

    getConfigContent: async () => client.get<ConfigContentResponse>('/storage/config/content'),

    saveConfigContent: async (content) => {
      await client.requestVoid('PUT', '/storage/config/content', { content })
    },

    analyzeConfig: async (content) =>
      client.post<ConfigAnalysisResponse>('/storage/config/analyze', {
        new_content: content,
      }),

    getStoragePaths: async () => client.get<StoragePathsResponse>('/storage/paths'),

    getDefaultDataDir: async () => client.get<string>('/storage/default-data-dir'),

    testStorageConnection: async (config) =>
      client.post<{ success: boolean; message: string }>('/storage/test-connection', config),

    executeMigration: async (plan, options) =>
      client.post<MigrationResult>('/storage/migration/execute', {
        plan,
        options: options ?? {},
      }),

    factoryReset: async () => client.post('/factory-reset'),
  }
}
