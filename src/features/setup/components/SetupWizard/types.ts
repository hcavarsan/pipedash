import type { MigrationResult, StorageConfigResponse } from '../../../../types/domain'

import type { SetupFieldErrors } from './setupDraft'

export type StorageBackend = 'sqlite' | 'postgres'

export type WizardStep = 'storage' | 'transfer' | 'confirm' | 'success'

export interface SetupState {
  backend: StorageBackend
  dataDir: string
  postgresUrl: string
  vaultPassword: string
  vaultPasswordConfirm: string
  transferData: boolean
}

export interface SetupWizardProps {
  opened: boolean
  onComplete: () => void
  onClose?: () => void
}

export interface VaultPasswordStatus {
  is_set: boolean
  env_var_name: string
}

export interface StorageStepProps {
  state: SetupState
  setState: (state: SetupState) => void
  currentConfig: StorageConfigResponse | null
  vaultStatus: VaultPasswordStatus
  error: string | null
  fieldErrors: SetupFieldErrors
}

export interface TransferStepProps {
  state: SetupState
  setState: (state: SetupState) => void
  error: string | null
}

export interface ConfirmStepProps {
  state: SetupState
  showTransferStep: boolean
  vaultPasswordFromEnv: boolean
  isMobile: boolean
  error: string | null
  vaultStatus: VaultPasswordStatus
}

export interface SuccessStepProps {
  state: SetupState
  migrationStats: MigrationResult['stats'] | null
  vaultStatus: VaultPasswordStatus
  vaultPasswordFromEnv: boolean
  onComplete: () => void
}
