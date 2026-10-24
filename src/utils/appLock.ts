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

// With "lock when leaving the app", the cover stays up across foregrounding until the lock reaches the UI.
export const nextCoverLatch = (
  latched: boolean,
  appLock: VaultStatusResponse['app_lock'],
  visibility: DocumentVisibilityState
): boolean =>
  appLock?.enabled === true && appLock.lock_on_background && !appLock.locked && (latched || visibility === 'hidden')

export const shouldShowPrivacyCover = (
  appLockEnabled: boolean,
  visibility: DocumentVisibilityState,
  latched: boolean
): boolean => appLockEnabled && (visibility === 'hidden' || latched)

export const isBiometricCancel = (message: string): boolean => message.toLowerCase().includes('cancel')
