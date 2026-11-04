import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'

import type { QueryClient } from '@tanstack/react-query'
import { QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'

import { desktopNotificationsKey } from '../../features/notifications/queries/useDesktopNotifications'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'

import { callbacks, delivered, listeners, registration } from './notificationEnvironment'
import { failedNotification } from './notificationPayloads'

export function defineRegistrationScenarios(client: QueryClient, Screen: () => ReactNode): void {
  afterEach(() => {
    registration.mode = 'immediate'
    registration.gates.length = 0
  })

  test('a rejected listener registration is survivable and delivers nothing', async (context) => {
    const warn = context.mock.method(console, 'warn', () => undefined)
    registration.mode = 'reject'

    await act(async () => {
      render(createElement(QueryClientProvider, { client }, createElement(Screen)))
    })

    await waitFor(() => assert.ok(warn.mock.calls.length >= 1))
    assert.equal(listeners.size, 0)
    assert.deepEqual(useNotificationCenterStore.getState().items, [])
    assert.equal(client.getQueryData(desktopNotificationsKey), undefined)
    assert.deepEqual(delivered, [])
  })

  test('an event delivered after unmount but before registration settles is dropped and the listener is removed', async () => {
    registration.mode = 'deferred'

    await act(async () => {
      render(createElement(QueryClientProvider, { client }, createElement(Screen)))
    })
    await waitFor(() => assert.ok(registration.gates.length >= 1))
    await act(async () => cleanup())

    await act(async () => {
      for (const [id, callback] of callbacks) {
        callback({ event: 'notification', id, payload: failedNotification })
      }
    })
    assert.deepEqual(useNotificationCenterStore.getState().items, [])
    assert.equal(client.getQueryData(desktopNotificationsKey), undefined)
    assert.deepEqual(delivered, [])

    await act(async () => {
      for (const gate of registration.gates) {
        gate.resolve(undefined)
      }
    })
    await waitFor(() => assert.equal(listeners.size, 0))
  })
}
