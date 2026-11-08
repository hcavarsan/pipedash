import type { SetupDraft } from '../../../../types/setup'
import type { StorageConfigResponse } from '../../../../types/storage'

import type { SetupFieldErrors, SetupState } from './types'

const FIELD_ERROR_KEYS: Record<string, keyof SetupFieldErrors> = {
  postgres_url: 'postgresUrl',
  vault_password: 'vaultPassword',
  vault_password_confirm: 'vaultPasswordConfirm',
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

export function toSetupDraft(state: SetupState): SetupDraft {
  return {
    backend: state.backend,
    data_dir: state.dataDir,
    postgres_url: state.postgresUrl,
    vault_password: state.vaultPassword,
    vault_password_confirm: state.vaultPasswordConfirm,
    transfer_data: state.transferData,
  }
}

export function toFieldErrors(errors: Record<string, string>): SetupFieldErrors {
  const fields: SetupFieldErrors = {}

  for (const [key, message] of Object.entries(errors)) {
    const field = FIELD_ERROR_KEYS[key]

    if (field) {
      fields[field] = message
    }
  }

  return fields
}
