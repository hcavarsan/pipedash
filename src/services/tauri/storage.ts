import { invoke } from '@tauri-apps/api/core'

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
import { invokeWithTimeout } from '../invoke'
import type { StorageService } from '../types'

export const storageTauriService: StorageService = {
  checkSetupStatus: async () => invoke<SetupStatus>('check_setup_status'),

  prepareSetupDraft: async (draft) => invoke<SetupDraftResult>('prepare_setup_draft', { draft }),

  applySetupDraft: async (draft) => invoke<SetupApplied>('apply_setup_draft', { draft }),

  getStorageConfig: async () => invoke<StorageConfigResponse>('get_storage_config'),

  getVaultPasswordStatus: async () => invoke<{ is_set: boolean; env_var_name: string }>('get_vault_password_status'),

  getVaultStatus: async () => invoke<VaultStatusResponse>('get_vault_status'),

  unlockVault: async (password) => invoke<UnlockVaultResponse>('unlock_vault', { password }),

  lockVault: async () => invoke<UnlockVaultResponse>('lock_vault'),

  getConfigContent: async () => invoke<ConfigContentResponse>('get_config_content'),

  saveConfigContent: async (content) => invoke<void>('save_config_content', { content }),

  analyzeConfig: async (content) => invoke<ConfigAnalysisResponse>('analyze_config', { content }),

  getStoragePaths: async () => invoke<StoragePathsResponse>('get_storage_paths'),

  getDefaultDataDir: async () => invoke<string>('get_default_data_dir'),

  testStorageConnection: async (config) =>
    invoke<{ success: boolean; message: string }>('test_storage_connection', { config }),

  executeMigration: async (plan, options) =>
    invoke<MigrationResult>('execute_storage_migration', { plan, options: options ?? {} }),

  factoryReset: async () => invokeWithTimeout('factory_reset'),
}
