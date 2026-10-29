import type { Biometry, VaultStatusResponse } from '../types/domain'

import { isAndroidPlatform } from './platform'

type LockState = Pick<VaultStatusResponse, 'is_unlocked' | 'requires_password' | 'app_lock'>

export const biometryLabel = (biometry: Biometry | undefined, android = isAndroidPlatform()): string => {
  switch (biometry) {
    case 'faceId':
      return android ? 'face unlock' : 'Face ID'
    case 'touchId':
      return android ? 'fingerprint' : 'Touch ID'
    default:
      return android ? 'screen lock' : 'device passcode'
  }
}

export const lockMeansSomething = (status: Pick<VaultStatusResponse, 'requires_password' | 'app_lock'>): boolean =>
  status.requires_password || status.app_lock?.enabled === true

export const needsUnlock = (status: LockState, tauri: boolean, hasToken: boolean): boolean => {
  if (!status.requires_password && status.app_lock?.locked !== true) {
    return false
  }

  return !status.is_unlocked || (!tauri && !hasToken)
}

export const shouldShowPrivacyCover = (appLockEnabled: boolean, visibility: DocumentVisibilityState): boolean =>
  appLockEnabled && visibility === 'hidden'
