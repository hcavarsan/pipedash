import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()
after(() => GlobalRegistrator.unregister())

const {
  DEFAULT_NOTIFICATION_PREFERENCES,
  migrateNotificationSettings,
  notificationAllowed,
  remapPreferences,
  useNotificationStore,
} = await import('./notificationStore')

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

test('a storage migration moves mutes to the new provider IDs and drops removed providers', () => {
  const preferences = {
    ...DEFAULT_NOTIFICATION_PREFERENCES,
    muted_providers: [2, 5],
    muted_pipelines: ['github__2__acme/api__ci.yml', 'gitlab__5__group/app', 'broken'],
  }

  assert.deepEqual(remapPreferences(preferences, { 2: 1, 3: 2 }), {
    ...preferences,
    muted_providers: [1],
    muted_pipelines: ['github__1__acme/api__ci.yml'],
  })
})

const deferredRemap = () => {
  const preferences = { ...DEFAULT_NOTIFICATION_PREFERENCES, muted_providers: [2, 5] }
  useNotificationStore.setState({ preferences })
  useNotificationStore.getState().remapProvidersOnRestart({ 2: 1, 3: 2 })
  assert.deepEqual(useNotificationStore.getState().preferences, preferences)
}
const mutedProviders = () => useNotificationStore.getState().preferences.muted_providers

test('a local-core migration keeps the old mutes until the next launch, then remaps them once', async () => {
  deferredRemap()

  await useNotificationStore.persist.rehydrate()
  assert.deepEqual(mutedProviders(), [1])

  await useNotificationStore.persist.rehydrate()
  assert.deepEqual(mutedProviders(), [1])
})

test('a phone remaps its mutes as soon as the core reloads on the new database', async () => {
  Object.assign(window, { __TAURI_INTERNALS__: { invoke: async () => undefined } })
  const { tauriService } = await import('../services/tauri')
  deferredRemap()

  await tauriService.restartApp()
  assert.deepEqual(mutedProviders(), [1])

  await useNotificationStore.persist.rehydrate()
  assert.deepEqual(mutedProviders(), [1])
})
