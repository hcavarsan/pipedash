import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { useState } from 'react'
import type { PendingNativeIntent } from '../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

let pending: PendingNativeIntent | null = null
let peekGate: PromiseWithResolvers<PendingNativeIntent | null> | null = null
let registration: PromiseWithResolvers<void> | null = null
let onAck: (() => void) | null = null
let failPeek = false
const calls: string[] = []
const acknowledged: number[] = []
const callbacks = new Map<number, (event: unknown) => void>()
let handler = 0

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: (callback: (event: unknown) => void) => {
      callbacks.set(++handler, callback)
      return handler
    },
    invoke: async (command: string, args: Record<string, unknown> = {}) => {
      calls.push(command)
      if (command === 'plugin:event|listen') {
        await registration?.promise
        return args.handler
      }
      if (command === 'native_peek_pending_intent') {
        if (failPeek) {
          throw new Error('Native bridge unavailable')
        }
        return peekGate ? peekGate.promise : pending
      }
      if (command === 'native_ack_pending_intent') {
        acknowledged.push(Number(args.id))
        onAck?.()
        onAck = null
        if (pending?.id === args.id) {
          pending = null
        }
      }
      return undefined
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { NativeBridge } = await import('./NativeBridge')
const { NativeIntentBridge } = await import('./NativeIntentBridge')
const { useModalStore } = await import('../stores/modalStore')
const { usePipelineViewStore } = await import('../features/pipelines/stores/pipelineViewStore')
const { useNotificationCenterStore } = await import('../stores/notificationCenterStore')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

afterEach(async () => {
  await act(async () => cleanup())
  pending = null
  peekGate = null
  registration = null
  onAck = null
  failPeek = false
  calls.length = 0
  acknowledged.length = 0
  callbacks.clear()
  useModalStore.setState(useModalStore.getInitialState(), true)
  usePipelineViewStore.setState(usePipelineViewStore.getInitialState(), true)
  useNotificationCenterStore.setState({ items: [], selection: null, desktopRevision: -1 })
})

const view = () => within(document.body)

function Harness({ authenticated = true }: { authenticated?: boolean }) {
  const [mounted, setMounted] = useState(authenticated)
  const { pathname, search } = useLocation()

  return (
    <>
      <NativeBridge />
      {mounted && <NativeIntentBridge />}
      <output aria-label="Location">
        {pathname}
        {search}
      </output>
      <button type="button" onClick={() => setMounted(!mounted)}>
        {mounted ? 'Lock' : 'Unlock'}
      </button>
    </>
  )
}

function mount(authenticated = true) {
  render(
    <MemoryRouter initialEntries={['/pipelines']}>
      <Harness authenticated={authenticated} />
    </MemoryRouter>
  )
}

test('registers the listener before peeking and handles the accepted intent once', async () => {
  registration = Promise.withResolvers<void>()
  pending = { id: 1, intent: { type: 'link', path: '/settings/general' } }
  mount()
  await act(async () => {})
  assert.equal(calls.includes('native_peek_pending_intent'), false)
  await act(async () => registration?.resolve())
  await waitFor(() => assert.deepEqual(acknowledged, [1]))
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/settings/general')
})

test('a selected exact run remains pending through unlock and opens once', async () => {
  const user = userEvent.setup({ document, delay: null })
  pending = {
    id: 7,
    intent: { type: 'notification', action: 'open', pipelineId: 'jenkins__1__menu-01', runNumber: 42 },
  }
  mount(false)
  await act(async () => {})
  assert.equal(calls.includes('native_peek_pending_intent'), false)
  assert.deepEqual(useModalStore.getState().logsModal, { open: false })
  await user.click(view().getByRole('button', { name: 'Unlock' }))
  await waitFor(() => assert.deepEqual(acknowledged, [7]))
  assert.deepEqual(useModalStore.getState().logsModal, { open: true, pipelineId: 'jenkins__1__menu-01', runNumber: 42 })
  assert.equal(pending, null)
})

for (const [action, destination] of [
  ['tap', '/notifications'],
  ['open-run', '/pipelines/jenkins__1__menu-01'],
] as const) {
  test(`a known pipeline notice routes ${action} to its intended destination without guessing a run`, async () => {
    const key = 'pipeline:known'
    useNotificationCenterStore.setState({
      items: [
        {
          key,
          at: 1,
          read: false,
          tone: 'danger',
          level: null,
          title: 'Pipeline failed',
          body: 'menu-01',
          providerIds: [1],
          pipelineId: 'jenkins__1__menu-01',
          link: null,
        },
      ],
    })
    pending = { id: 12, intent: { type: 'notification', action, key, pipelineId: 'jenkins__1__menu-01' } }
    mount()
    await waitFor(() => assert.deepEqual(acknowledged, [12]))
    assert.equal(view().getByRole('status', { name: 'Location' }).textContent, destination)
    assert.deepEqual(useModalStore.getState().logsModal, { open: false })
    assert.deepEqual(
      useNotificationCenterStore.getState().selection,
      action === 'tap' ? { type: 'notification', key } : null
    )
  })
}

test('unmounting during peek retains the intent for the next authenticated mount', async () => {
  const user = userEvent.setup({ document, delay: null })
  pending = { id: 2, intent: { type: 'link', path: '/notifications' } }
  peekGate = Promise.withResolvers<PendingNativeIntent | null>()
  mount()
  await waitFor(() => assert.ok(calls.includes('native_peek_pending_intent')))
  await user.click(view().getByRole('button', { name: 'Lock' }))
  await act(async () => peekGate?.resolve(pending))
  assert.deepEqual(acknowledged, [])
  assert.equal(pending?.id, 2)
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/pipelines')
  peekGate = null
  await user.click(view().getByRole('button', { name: 'Unlock' }))
  await waitFor(() => assert.deepEqual(acknowledged, [2]))
})

test('acknowledging an older intent preserves and drains its replacement', async () => {
  pending = { id: 3, intent: { type: 'link', path: '/settings/general' } }
  onAck = () => {
    pending = { id: 4, intent: { type: 'link', path: '/notifications' } }
  }
  mount()
  await waitFor(() => assert.deepEqual(acknowledged, [3, 4]))
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/notifications')
})

test('a failed peek waits for focus instead of looping or discarding the intent', async (context) => {
  context.mock.method(console, 'warn', () => undefined)
  failPeek = true
  pending = { id: 5, intent: { type: 'link', path: '/notifications' } }
  mount()
  await waitFor(() => assert.ok(calls.includes('native_peek_pending_intent')))
  assert.deepEqual(acknowledged, [])
  assert.equal(calls.filter((command) => command === 'native_peek_pending_intent').length, 1)
  failPeek = false
  act(() => window.dispatchEvent(new Event('focus')))
  await waitFor(() => assert.deepEqual(acknowledged, [5]))
})

for (const pipelineId of ['owner/repo', 'percent%2Fname', 'hash#branch', '日本語/ação']) {
  test(`structured run links decode ${pipelineId} once without including query text`, async () => {
    pending = {
      id: 8,
      intent: { type: 'link', path: `/pipelines/${encodeURIComponent(pipelineId)}?tab=history`, runNumber: 42 },
    }
    mount()
    await waitFor(() => assert.deepEqual(acknowledged, [8]))
    assert.deepEqual(useModalStore.getState().logsModal, { open: true, pipelineId, runNumber: 42 })
    assert.ok(view().getByRole('status', { name: 'Location' }).textContent?.endsWith('?tab=history'))
  })
}

test('Manage pins opens the existing pinned editor on the pinned route', async () => {
  pending = { id: 9, intent: { type: 'ui', action: 'managePins' } }
  mount()
  await waitFor(() => assert.deepEqual(acknowledged, [9]))
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/pipelines?view=pinned')
  assert.equal(usePipelineViewStore.getState().editor.viewId, 'pinned')
  assert.equal(usePipelineViewStore.getState().editor.open, true)
})

test('Manage views opens the existing manager', async () => {
  pending = { id: 10, intent: { type: 'ui', action: 'manageViews' } }
  mount()
  await waitFor(() => assert.deepEqual(acknowledged, [10]))
  assert.equal(usePipelineViewStore.getState().manageOpen, true)
})

for (const [target, destination] of [
  [{ key: 'desktop-test' }, '/notifications'],
  [{ providerId: 3 }, '/settings/providers'],
  [{ pipelineId: 'gone/pipeline' }, '/pipelines/gone%2Fpipeline'],
] as const) {
  test(`notification fallback opens ${destination}`, async () => {
    pending = { id: 11, intent: { type: 'notification', action: 'tap', ...target } }
    mount()
    await waitFor(() => assert.deepEqual(acknowledged, [11]))
    assert.equal(view().getByRole('status', { name: 'Location' }).textContent, destination)
    assert.deepEqual(useModalStore.getState().logsModal, { open: false })
  })
}
