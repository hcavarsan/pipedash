import assert from 'node:assert/strict'
import { after, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient } from '@tanstack/react-query'

GlobalRegistrator.register()

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { useEventBatch } = await import('./useEventBatch')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

test('a steady stream of events still invalidates once per batch window', () => {
  const client = new QueryClient()
  const invalidate = mock.method(client, 'invalidateQueries', async () => undefined)
  const { result } = renderHook(() => useEventBatch(client))

  mock.timers.enable({ apis: ['setTimeout'] })
  try {
    for (let elapsed = 0; elapsed < 300; elapsed += 100) {
      result.current.scheduleBatchedInvalidation()
      mock.timers.tick(100)
    }

    assert.equal(invalidate.mock.callCount(), 1)

    for (let elapsed = 300; elapsed < 1000; elapsed += 100) {
      result.current.scheduleBatchedInvalidation()
      mock.timers.tick(100)
    }

    assert.equal(invalidate.mock.callCount(), 3)
  } finally {
    mock.timers.reset()
  }
})
