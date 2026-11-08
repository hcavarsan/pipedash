import type { StorageBackendType } from './storage'

export type PasswordSource = 'env_var' | 'keyring' | 'session' | 'none'

export type Biometry = 'none' | 'touchId' | 'faceId'

export interface AppLockStatus {
  supported: boolean
  biometry: Biometry
  biometry_available: boolean
  enabled: boolean
  lock_on_background: boolean
  locked: boolean
}

export interface VaultStatusResponse {
  is_unlocked: boolean
  password_source: PasswordSource
  backend: StorageBackendType
  requires_password: boolean
  is_first_time?: boolean
  app_lock?: AppLockStatus
  demo_password?: string
}

export interface UnlockVaultResponse {
  success: boolean
  message: string
  reason?: 'cancelled'
}
