import assert from 'node:assert/strict'
import { beforeEach, test } from 'node:test'

import { MAX_CENTER_NOTIFICATIONS, type NotificationInput, useNotificationCenterStore } from './notificationCenterStore'

const input = (overrides: Partial<NotificationInput> = {}): NotificationInput => ({
  key: 'status-page:github',
  at: 1,
  tone: 'warning',
  level: 'major_outage',
  title: 'Actions is down',
  body: 'Investigating',
  providerIds: [1],
  pipelineId: null,
  link: null,
  ...overrides,
})

beforeEach(() => {
  useNotificationCenterStore.setState({ items: [], selection: null })
})

test('a notification with the same key replaces the earlier one and moves to the top', () => {
  const { push } = useNotificationCenterStore.getState()

  push(input())
  push(input({ key: 'connection:2', title: "Can't reach Gitea" }))
  const recoveryId = push(input({ tone: 'success', title: 'GitHub is back to normal' }))

  const { items } = useNotificationCenterStore.getState()

  assert.deepEqual(
    items.map((item) => item.title),
    ['GitHub is back to normal', "Can't reach Gitea"]
  )
  assert.equal(items[0]?.id, recoveryId)
  assert.equal(items[0]?.read, false)
})

test('opening a notification marks only that one as read', () => {
  const { push, open } = useNotificationCenterStore.getState()
  const first = push(input())
  push(input({ key: 'connection:2' }))

  open({ type: 'notification', id: first })

  const { items, selection } = useNotificationCenterStore.getState()

  assert.deepEqual(selection, { type: 'notification', id: first })
  assert.deepEqual(
    items.map((item) => [item.id === first, item.read]),
    [
      [false, false],
      [true, true],
    ]
  )
})

test('mark all read keeps the items and close clears the selection', () => {
  const { push, markAllRead, open, close } = useNotificationCenterStore.getState()
  push(input())
  push(input({ key: 'connection:2' }))

  markAllRead()
  open({ type: 'provider', providerId: 1 })
  close()

  const { items, selection } = useNotificationCenterStore.getState()

  assert.equal(items.length, 2)
  assert.ok(items.every((item) => item.read))
  assert.equal(selection, null)
})

test('the list keeps only the newest notifications', () => {
  const { push } = useNotificationCenterStore.getState()

  for (let index = 0; index <= MAX_CENTER_NOTIFICATIONS; index += 1) {
    push(input({ key: `pipeline:${index}`, title: `run ${index}` }))
  }

  const { items } = useNotificationCenterStore.getState()

  assert.equal(items.length, MAX_CENTER_NOTIFICATIONS)
  assert.equal(items[0]?.title, `run ${MAX_CENTER_NOTIFICATIONS}`)
  assert.equal(items.at(-1)?.title, 'run 1')
})
