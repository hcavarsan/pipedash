import assert from 'node:assert/strict'
import { test } from 'node:test'

import { DEFAULT_NOTIFICATION_PREFERENCES, migrateNotificationSettings, notificationAllowed } from './notificationStore'

const enabled = (notificationsEnabled: boolean) => ({
  notificationsEnabled,
  preferences: DEFAULT_NOTIFICATION_PREFERENCES,
})

test('either legacy toggle keeps notifications on', () => {
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: true, providerAlertsEnabled: false }, 0),
    enabled(true)
  )
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: false, providerAlertsEnabled: true }, 0),
    enabled(true)
  )
})

test('both legacy toggles off, or nothing saved, stays off', () => {
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: false, providerAlertsEnabled: false }, 0),
    enabled(false)
  )
  assert.deepEqual(migrateNotificationSettings(undefined, 0), enabled(false))
})

test('the unified flag is kept once saved and ignores stray legacy keys', () => {
  assert.deepEqual(migrateNotificationSettings({ notificationsEnabled: true }, 1), enabled(true))
  assert.deepEqual(
    migrateNotificationSettings({ notificationsEnabled: false, providerAlertsEnabled: true }, 1),
    enabled(false)
  )
})

test('saved preferences keep what was chosen and drop malformed entries', () => {
  const { preferences } = migrateNotificationSettings(
    {
      notificationsEnabled: true,
      preferences: { long_run: false, outage: 'no', muted_providers: [3, '4', 1.5], muted_pipelines: ['a', 7] },
    },
    2
  )

  assert.deepEqual(preferences, {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    long_run: false,
    muted_providers: [3],
    muted_pipelines: ['a'],
  })
})

test('a notification passes only when its kind is on and its source is not muted', () => {
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    recovered: false,
    muted_providers: [2],
    muted_pipelines: ['github__1__api'],
  }

  assert.equal(notificationAllowed(preferences, 'failed', [1], 'github__1__web'), true)
  assert.equal(notificationAllowed(preferences, 'failed', [1], 'github__1__api'), false)
  assert.equal(notificationAllowed(preferences, 'recovered', [1], null), false)
  assert.equal(notificationAllowed(preferences, 'outage', [2], null), false)
  assert.equal(notificationAllowed(preferences, 'outage', [2, 3], null), true)
  assert.equal(notificationAllowed(preferences, 'outage', [], null), true)
  assert.equal(notificationAllowed(preferences, 'test', [2], 'github__1__api'), true)
})
