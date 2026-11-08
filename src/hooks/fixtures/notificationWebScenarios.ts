import './notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement } from 'react'
import { wsClient } from '../../services/websocket'
import { useAuthStore } from '../../stores/authStore'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } from '../../stores/notificationStore'
import { installNotificationCenterDouble } from '../../testing/notificationCenter'
import { isRecord } from '../../utils/isRecord'
import { useNotificationEvents } from '../useNotificationEvents'
import { failedNotification, outageNotification } from './notificationPayloads'
import { delivered, Socket } from './notificationWebEnvironment'

const watch = { pipeline_id: failedNotification.pipeline_id ?? '', since: '2026-10-26T11:00:00.000Z' }

function Screen() {
  useNotificationEvents()
  return null
}

async function mount() {
  useNotificationStore.setState({ notificationsEnabled: true })
  await act(async () => {
    render(createElement(Screen))
  })
  const socket = Socket.instances.at(-1)
  assert.ok(socket)
  socket.open()
  return socket
}

const frames = (socket: Socket): unknown[] => socket.sent.map((data) => JSON.parse(data))

const preferenceFrames = (socket: Socket): unknown[] =>
  frames(socket).filter((frame) => isRecord(frame) && frame.type === 'notification-preferences')

const emit = (socket: Socket, type: string, payload: unknown) =>
  act(async () => {
    socket.emit(type, payload)
  })

const reconnect = () => {
  wsClient.disconnect()
  wsClient.manualReconnect()
  const socket = Socket.instances.at(-1)
  assert.ok(socket)
  socket.open()
  return socket
}

installNotificationCenterDouble()

afterEach(async () => {
  await act(async () => cleanup())
  wsClient.dispose()
  useAuthStore.getState().clearToken()
  useNotificationCenterStore.setState({ items: [], revision: 0, unread: 0, selection: null })
  Socket.instances.length = 0
  delivered.length = 0
  useNotificationStore.setState({ notificationsEnabled: false, preferences: DEFAULT_NOTIFICATION_PREFERENCES })
})

after(() => GlobalRegistrator.unregister())

test('a notification frame reaches the center and the browser', async () => {
  const socket = await mount()
  await emit(socket, 'notification', failedNotification)
  const [item] = useNotificationCenterStore.getState().items
  assert.equal(item?.pipelineId, failedNotification.pipeline_id)
  assert.equal(item?.tone, 'danger')
  await waitFor(() => assert.deepEqual(delivered, [{ title: failedNotification.title, body: failedNotification.body }]))
})

test('an outage frame carries its level and link into the center', async () => {
  const socket = await mount()
  await emit(socket, 'notification', outageNotification)
  const [item] = useNotificationCenterStore.getState().items
  assert.equal(item?.level, 'major_outage')
  assert.deepEqual(item?.link, outageNotification.link)
  assert.deepEqual(item?.providerIds, [1])
  await waitFor(() => assert.equal(delivered.length, 1))
})

test('with notifications off a frame stays in the center only', async () => {
  const socket = await mount()
  useNotificationStore.setState({ notificationsEnabled: false })
  await emit(socket, 'notification', failedNotification)
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
  assert.deepEqual(delivered, [])
})

test('nothing is shown without a notification frame', async () => {
  const socket = await mount()
  await emit(socket, 'pipelines-failed', { pipelines: [] })
  await emit(socket, 'pipelines-finished', { pipelines: [] })
  await emit(socket, 'provider-health-alert', {})
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  assert.deepEqual(delivered, [])
})

test('a fired one-time request in a frame is marked used', async () => {
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [watch] } })
  const socket = await mount()
  const fired = { ...watch, fired_at: '2026-10-26T12:01:00.000Z' }
  await emit(socket, 'notification', { ...failedNotification, fired })
  const { preferences } = useNotificationStore.getState()
  assert.deepEqual(preferences.watch_once, [fired])
  assert.deepEqual(
    preferences.watch_once.filter((item) => !item.fired_at),
    []
  )
})

test('the preferences go out when the socket opens and whenever they change', async () => {
  const socket = await mount()
  assert.deepEqual(preferenceFrames(socket), [
    { type: 'notification-preferences', preferences: DEFAULT_NOTIFICATION_PREFERENCES },
  ])

  await act(async () => {
    useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, failed: false } })
  })
  assert.deepEqual(preferenceFrames(socket).at(-1), {
    type: 'notification-preferences',
    preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, failed: false },
  })
  assert.equal(preferenceFrames(socket).length, 2)
})

test('after reconnecting the preferences carry the last notification id received', async () => {
  const first = await mount()
  await emit(first, 'notification', { ...failedNotification, id: 6 })
  await emit(first, 'notification', { ...outageNotification, id: 7 })

  const second = reconnect()

  assert.deepEqual(preferenceFrames(second), [
    { type: 'notification-preferences', preferences: DEFAULT_NOTIFICATION_PREFERENCES, after: 7 },
  ])
  assert.deepEqual(preferenceFrames(first), [
    { type: 'notification-preferences', preferences: DEFAULT_NOTIFICATION_PREFERENCES },
  ])
})

test('with a token the preferences wait for the server to accept it, then go out again after each reauthentication', async () => {
  useAuthStore.getState().setToken('secret')
  const socket = await mount()
  assert.deepEqual(frames(socket), [{ type: 'auth', token: 'secret' }])

  await emit(socket, 'auth-ok', { host: 'server', name: null, deviceId: null })
  assert.deepEqual(frames(socket), [
    { type: 'auth', token: 'secret' },
    { type: 'notification-preferences', preferences: DEFAULT_NOTIFICATION_PREFERENCES },
  ])

  await emit(socket, 'notification', { ...failedNotification, id: 3 })
  await act(async () => wsClient.reauthenticate())
  await emit(socket, 'auth-ok', { host: 'server', name: null, deviceId: null })
  assert.deepEqual(preferenceFrames(socket).at(-1), {
    type: 'notification-preferences',
    preferences: DEFAULT_NOTIFICATION_PREFERENCES,
    after: 3,
  })
})

test('preferences changed before the server accepts the token are not sent', async () => {
  useAuthStore.getState().setToken('wrong')
  const socket = await mount()
  await act(async () => {
    useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, failed: false } })
  })
  assert.deepEqual(preferenceFrames(socket), [])
})
