import type { StorageConfigResponse } from '../../../types'

export type StorageBackend = 'sqlite' | 'postgres'

export interface SetupState {
  backend: StorageBackend;
  dataDir: string;
  postgresUrl: string;
  vaultPassword: string;
  vaultPasswordConfirm: string;
  transferData: boolean;
}

export interface SetupWizardProps {
  opened: boolean;
  onComplete: () => void;
  onClose?: () => void;
}

export interface VaultPasswordStatus {
  is_set: boolean;
  env_var_name: string;
}

export interface TransferResult {
  success: boolean;
  message: string;
}

export interface StorageStepProps {
  state: SetupState;
  setState: (state: SetupState) => void;
  currentConfig: StorageConfigResponse | null;
  vaultStatus: VaultPasswordStatus | null;
  defaultDataDir: string;
  error: string | null;
  hasAnyChange: boolean;
  checkingVault: boolean;
}

export interface TransferStepProps {
  state: SetupState;
  setState: (state: SetupState) => void;
  isDatabaseBackendChange: boolean;
  isDataDirChange: boolean;
  isFromKeyring: boolean;
  getCurrentBackend: () => string;
  getCurrentDataDir: () => string;
  error: string | null;
}

export interface ConfirmStepProps {
  state: SetupState;
  isDatabaseBackendChange: boolean;
  isFromKeyring: boolean;
  isDataDirChange: boolean;
  showTransferStep: boolean;
  vaultPasswordFromEnv: boolean;
  needsVaultPassword: boolean;
  isMobile: boolean;
  error: string | null;
  getCurrentBackend: () => string;
  getCurrentDataDir: () => string;
}

export interface SuccessStepProps {
  state: SetupState;
  migrationCompleted: boolean;
  transferResult: TransferResult | null;
  vaultStatus: VaultPasswordStatus | null;
  vaultPasswordFromEnv: boolean;
  onComplete: () => void;
}
