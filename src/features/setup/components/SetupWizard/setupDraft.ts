import type { PipedashConfig, StorageConfigResponse } from '../../../../types/domain'

import type { SetupState } from './types'

export type SetupFieldErrors = Partial<Record<'postgresUrl' | 'vaultPassword' | 'vaultPasswordConfirm', string>>

export function isPostgresUrl(value: string): boolean {
  try {
    const url = new URL(value.trim())

    return (url.protocol === 'postgres:' || url.protocol === 'postgresql:') && Boolean(url.hostname)
  } catch {
    return false
  }
}

export function validateSetupDraft(draft: SetupState, needsVaultPassword: boolean): SetupFieldErrors {
  const errors: SetupFieldErrors = {}

  if (draft.backend === 'postgres' && !isPostgresUrl(draft.postgresUrl)) {
    errors.postgresUrl = 'Enter a valid PostgreSQL connection URL'
  }

  if (needsVaultPassword) {
    if (draft.vaultPassword.length < 8) {
      errors.vaultPassword = 'Use at least 8 characters'
    }
    if (draft.vaultPassword !== draft.vaultPasswordConfirm) {
      errors.vaultPasswordConfirm = 'Passwords do not match'
    }
  }

  return errors
}

export function initialDraft(currentConfig: StorageConfigResponse | null, defaultDataDir: string): SetupState {
  const storage = currentConfig?.config.storage

  return {
    backend: storage?.backend || 'sqlite',
    dataDir: storage?.data_dir || defaultDataDir,
    postgresUrl: storage?.postgres?.connection_string || '',
    vaultPassword: '',
    vaultPasswordConfirm: '',
    transferData: true,
  }
}

export function buildStorageConfig(
  draft: SetupState,
  currentConfig: StorageConfigResponse | null,
  useDefaultDir: boolean
): PipedashConfig {
  return {
    general: currentConfig?.config.general || {
      metrics_enabled: true,
      default_refresh_interval: 30,
      status_pages_enabled: true,
      proxy_url: '',
    },
    server: currentConfig?.config.server || {
      bind_addr: '127.0.0.1:8080',
      cors_allow_all: false,
    },
    storage: {
      data_dir: useDefaultDir ? '' : draft.dataDir,
      backend: draft.backend,
      ...(draft.backend === 'postgres' && {
        postgres: {
          connection_string: draft.postgresUrl,
        },
      }),
    },
  }
}
