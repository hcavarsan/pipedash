import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { usePinning } = await import('./usePinning')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

test('a sidebar pin item is disabled while its request is pending and ignores a repeat activation', async (context) => {
  const hold = Promise.withResolvers<void>()
  context.mock.method(apiService, 'listPins', async () => [])
  const setPin = context.mock.method(apiService, 'setPin', () => hold.promise)
  const { result } = renderHook(() => usePinning(), { wrapper })
  const item = result.current('view', 'v1')

  assert.equal(item.disabled, false)
  await act(async () => {
    item.onClick()
    item.onClick()
  })

  assert.equal(setPin.mock.callCount(), 1)
  await waitFor(() => assert.equal(result.current('view', 'v1').disabled, true))
  assert.equal(result.current('view', 'v2').disabled, false)
  assert.equal(result.current('repository', 'v1').disabled, false)

  await act(async () => result.current('view', 'v1').onClick())
  assert.equal(setPin.mock.callCount(), 1)

  hold.resolve()
  await waitFor(() => assert.equal(result.current('view', 'v1').disabled, false))
})
