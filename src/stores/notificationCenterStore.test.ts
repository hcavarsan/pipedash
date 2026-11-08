import assert from 'node:assert/strict'
import { afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { CenterNotification, NotificationCenterChange, NotificationCenterSnapshot } from '../types/notifications'

GlobalRegistrator.register({ width: 1200, height: 800 })

const { apiService } = await import('../services/api')
const { useNotificationCenterStore } = await import('./notificationCenterStore')

const notice = (key: string, read = false): CenterNotification => ({
  key,
  at: 1,
  read,
  tone: 'warning',
  level: null,
  title: key,
  body: '',
  providerIds: [1],
  pipelineId: null,
  link: null,
})

const calls: { snapshot: NotificationCenterSnapshot; change: NotificationCenterChange }[] = []
let reply: NotificationCenterSnapshot = { revision: 0, items: [], unread: 0 }

beforeEach(() => {
  calls.length = 0
  reply = { revision: 1, items: [notice('a')], unread: 1 }
  useNotificationCenterStore.setState({ items: [], revision: 0, unread: 0, selection: null })
  mock.method(
    apiService,
    'updateNotificationCenter',
    async (snapshot: NotificationCenterSnapshot, change: NotificationCenterChange) => {
      calls.push({ snapshot, change })

      return reply
    }
  )
})

afterEach(() => mock.restoreAll())

test('a push sends the current snapshot and stores what the backend returns', async () => {
  await useNotificationCenterStore.getState().push({ ...notice('a'), at: 5 })

  const { items, revision, unread } = useNotificationCenterStore.getState()

  assert.deepEqual(calls[0]?.snapshot, { revision: 0, items: [], unread: 0 })
  assert.deepEqual(calls[0]?.change, { type: 'push', notice: { ...notice('a'), at: 5, read: false } })
  assert.deepEqual({ items, revision, unread }, reply)
})

test('changes are applied one after another on the latest snapshot', async () => {
  const { push, markAllRead } = useNotificationCenterStore.getState()

  await Promise.all([push(notice('a')), markAllRead()])

  assert.equal(calls.length, 2)
  assert.deepEqual(calls[1]?.snapshot, { revision: 1, items: [notice('a')], unread: 1 })
  assert.deepEqual(calls[1]?.change, { type: 'read', key: null })
})

test('opening a notification selects it and marks it read through the backend', async () => {
  useNotificationCenterStore.getState().open({ type: 'notification', key: 'a' })

  assert.deepEqual(useNotificationCenterStore.getState().selection, { type: 'notification', key: 'a' })
  await useNotificationCenterStore.getState().clear()
  assert.deepEqual(calls[0]?.change, { type: 'read', key: 'a' })
})

test('opening a provider only selects it and close clears the selection', () => {
  const { open, close } = useNotificationCenterStore.getState()

  open({ type: 'provider', providerId: 1 })
  assert.deepEqual(useNotificationCenterStore.getState().selection, { type: 'provider', providerId: 1 })
  assert.equal(calls.length, 0)

  close()
  assert.equal(useNotificationCenterStore.getState().selection, null)
})
