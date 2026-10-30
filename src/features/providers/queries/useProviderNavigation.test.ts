import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react'

import { providerSummaryFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

// Browser globals must exist before these modules initialize.
const { act, cleanup, fireEvent, render, renderHook } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { useNotificationCenterStore } = await import('../../../stores/notificationCenterStore')
const { useProviderNavigation } = await import('./useProviderNavigation')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(MemoryRouter, { initialEntries: ['/browse'] }, createElement(QueryClientProvider, { client }, children))

afterEach(async () => {
  await act(async () => cleanup())
  useNotificationCenterStore.getState().close()
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('a failed provider opens its page beneath status and stays there after dismissal', async () => {
  const provider = providerSummaryFixture({
    id: 7,
    last_fetch_status: 'error',
    last_fetch_error_kind: 'network',
    freshness: { state: 'down', stale_at: null },
  })
  const hook = renderHook(() => ({ open: useProviderNavigation(), location: useLocation() }), { wrapper })

  await act(async () => hook.result.current.open(provider))

  assert.equal(hook.result.current.location.pathname + hook.result.current.location.search, '/pipelines?provider=7')
  assert.equal(hook.result.current.location.state.from, '/browse')
  assert.deepEqual(useNotificationCenterStore.getState().selection, { type: 'provider', providerId: 7 })

  await act(async () => useNotificationCenterStore.getState().close())
  hook.rerender()

  assert.equal(useNotificationCenterStore.getState().selection, null)
  assert.equal(hook.result.current.location.search, '?provider=7')
})

test('a healthy provider navigates without opening recovery', async () => {
  const hook = renderHook(() => ({ open: useProviderNavigation(), location: useLocation() }), { wrapper })

  await act(async () => hook.result.current.open(providerSummaryFixture({ last_fetch_status: 'success' })))

  assert.equal(hook.result.current.location.search, '?provider=1')
  assert.equal(useNotificationCenterStore.getState().selection, null)
})

function ProviderLink() {
  const open = useProviderNavigation()
  const location = useLocation()

  return createElement(
    'a',
    {
      href: '/pipelines?provider=1',
      onClick: (event: ReactMouseEvent<HTMLAnchorElement>) =>
        open(providerSummaryFixture({ recovery: 'sign_in', last_fetch_status: 'error' }), event),
    },
    location.pathname
  )
}
test('modified provider links retain browser navigation without opening recovery in this tab', async () => {
  const view = render(createElement(ProviderLink), { wrapper })
  const event = new MouseEvent('click', { ctrlKey: true, cancelable: true, bubbles: true })

  await act(async () => fireEvent(view.getByRole('link'), event))

  assert.equal(event.defaultPrevented, false)
  assert.equal(view.getByRole('link').textContent, '/browse')
  assert.equal(useNotificationCenterStore.getState().selection, null)
})
