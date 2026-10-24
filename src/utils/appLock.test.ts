import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { AppLockStatus } from '../types'

import { biometryLabel, isBiometricCancel, lockMeansSomething, needsUnlock, shouldShowPrivacyCover } from './appLock'

const appLock = (overrides: Partial<AppLockStatus> = {}): AppLockStatus => ({
  supported: true,
  biometry: 'faceId',
  biometry_available: true,
  enabled: false,
  lock_on_background: false,
  locked: false,
  ...overrides,
})

test('the lock button shows only when locking protects something', () => {
  assert.equal(lockMeansSomething({ requires_password: false }), false)
  assert.equal(lockMeansSomething({ requires_password: false, app_lock: appLock() }), false)
  assert.equal(lockMeansSomething({ requires_password: true }), true)
  assert.equal(lockMeansSomething({ requires_password: false, app_lock: appLock({ enabled: true }) }), true)
})

test('keychain mode asks for unlock only while the app lock is engaged', () => {
  const open = { is_unlocked: true, requires_password: false, app_lock: appLock({ enabled: true }) }
  const locked = { is_unlocked: false, requires_password: false, app_lock: appLock({ enabled: true, locked: true }) }

  assert.equal(needsUnlock(open, true, false), false)
  assert.equal(needsUnlock(locked, true, false), true)
  assert.equal(needsUnlock({ is_unlocked: false, requires_password: false }, true, false), false)
})

test('encrypted vault asks for unlock until opened; the web also needs its token', () => {
  const vault = { is_unlocked: true, requires_password: true }

  assert.equal(needsUnlock(vault, true, false), false)
  assert.equal(needsUnlock(vault, false, false), true)
  assert.equal(needsUnlock(vault, false, true), false)
  assert.equal(needsUnlock({ ...vault, is_unlocked: false }, true, true), true)
})

test('the privacy cover needs app lock and a hidden page', () => {
  assert.equal(shouldShowPrivacyCover(true, 'hidden'), true)
  assert.equal(shouldShowPrivacyCover(true, 'visible'), false)
  assert.equal(shouldShowPrivacyCover(false, 'hidden'), false)
})

test('biometric labels and cancel detection', () => {
  assert.equal(biometryLabel('faceId'), 'Face ID')
  assert.equal(biometryLabel('touchId'), 'Touch ID')
  assert.equal(biometryLabel('none'), 'device passcode')
  assert.equal(biometryLabel(undefined), 'device passcode')
  assert.equal(isBiometricCancel('cancelled'), true)
  assert.equal(isBiometricCancel('User canceled'), true)
  assert.equal(isBiometricCancel('Face ID not enrolled'), false)
})
