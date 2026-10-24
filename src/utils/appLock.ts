import type { Biometry, VaultStatusResponse } from '../types'

type LockState = Pick<VaultStatusResponse, 'is_unlocked' | 'requires_password' | 'app_lock'>

export const biometryLabel = (biometry: Biometry | undefined): string => {
  switch (biometry) {
    case 'faceId':
      return 'Face ID'
    case 'touchId':
      return 'Touch ID'
    default:
      return 'device passcode'
  }
}

export const lockMeansSomething = (status: Pick<VaultStatusResponse, 'requires_password' | 'app_lock'>): boolean =>
  status.requires_password || status.app_lock?.enabled === true

// Tauri trusts the vault status alone; the web also needs the bearer token that unlocking stores.
export const needsUnlock = (status: LockState, tauri: boolean, hasToken: boolean): boolean => {
  if (!status.requires_password && status.app_lock?.locked !== true) {
    return false
  }

  return !status.is_unlocked || (!tauri && !hasToken)
}

export const shouldShowPrivacyCover = (appLockEnabled: boolean, visibility: DocumentVisibilityState): boolean =>
  appLockEnabled && visibility === 'hidden'

export const isBiometricCancel = (message: string): boolean => message.toLowerCase().includes('cancel')
