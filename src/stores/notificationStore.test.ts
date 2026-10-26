import assert from 'node:assert/strict'
import { test } from 'node:test'

import { migrateNotificationSettings } from './notificationStore'

test('either legacy toggle keeps notifications on', () => {
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: true, providerAlertsEnabled: false }, 0),
    {
      notificationsEnabled: true,
    }
  )
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: false, providerAlertsEnabled: true }, 0),
    {
      notificationsEnabled: true,
    }
  )
})

test('both legacy toggles off, or nothing saved, stays off', () => {
  assert.deepEqual(
    migrateNotificationSettings({ failureNotificationsEnabled: false, providerAlertsEnabled: false }, 0),
    {
      notificationsEnabled: false,
    }
  )
  assert.deepEqual(migrateNotificationSettings(undefined, 0), { notificationsEnabled: false })
})

test('the unified flag is kept once saved and ignores stray legacy keys', () => {
  assert.deepEqual(migrateNotificationSettings({ notificationsEnabled: true }, 1), { notificationsEnabled: true })
  assert.deepEqual(migrateNotificationSettings({ notificationsEnabled: false, providerAlertsEnabled: true }, 1), {
    notificationsEnabled: false,
  })
})
