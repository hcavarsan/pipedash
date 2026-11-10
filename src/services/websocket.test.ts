import '../hooks/fixtures/notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { Socket } from '../hooks/fixtures/notificationWebEnvironment'
import { getToken, useAuthStore } from '../stores/authStore'
import { wsClient } from './websocket'

const statuses: string[] = []

const track = () => wsClient.listen<{ status: string }>('connection-status', ({ status }) => statuses.push(status))

const lastSocket = (): Socket => {
  const socket = Socket.instances.at(-1)
  assert.ok(socket)

  return socket
}

afterEach(() => {
  mock.timers.reset()
  wsClient.dispose()
  useAuthStore.getState().clearToken()
  Socket.instances.length = 0
  statuses.length = 0
})

after(() => GlobalRegistrator.unregister())

test('ten short opens and closes keep the token and keep reconnecting', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  useAuthStore.getState().setToken('secret')
  track()

  for (let attempt = 0; attempt < 10; attempt++) {
    const socket = lastSocket()

    socket.open()
    socket.close()
    mock.timers.tick(30_000)
  }

  assert.equal(Socket.instances.length, 11)
  assert.equal(getToken(), 'secret')
  lastSocket().open()
  assert.deepEqual(JSON.parse(lastSocket().sent[0] ?? ''), { type: 'auth', token: 'secret' })
})

test('with a token, connected is emitted only after auth-ok', () => {
  useAuthStore.getState().setToken('secret')
  track()
  const socket = lastSocket()

  socket.open()
  assert.equal(statuses.includes('connected'), false)
  assert.equal(wsClient.isConnected(), false)

  socket.emit('auth-ok', {})
  assert.deepEqual(statuses, ['connected'])
  assert.equal(wsClient.isConnected(), true)
})

test('without a token, connected is emitted on open', () => {
  track()

  lastSocket().open()

  assert.deepEqual(statuses, ['connected'])
  assert.equal(wsClient.isConnected(), true)
})
