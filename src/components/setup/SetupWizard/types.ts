import type { MigrationResult, StorageConfigResponse } from '../../../types'

export type StorageBackend = 'sqlite' | 'postgres'

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
  vaultStatus: VaultPasswordStatus | null
  error: string | null
  checkingVault: boolean
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
  vaultStatus: VaultPasswordStatus | null
}

export interface SuccessStepProps {
  state: SetupState
  migrationStats: MigrationResult['stats'] | null
  vaultStatus: VaultPasswordStatus | null
  vaultPasswordFromEnv: boolean
  onComplete: () => void
}
