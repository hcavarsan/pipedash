import assert from 'node:assert/strict'
import { test } from 'node:test'

Object.assign(globalThis, { window: globalThis, __TAURI_INTERNALS__: { invoke: async () => 'done' } })

const { invokeWithTimeout } = await import('./invoke')

test('a finished command clears its timeout timer', async (context) => {
  const cleared = context.mock.method(globalThis, 'clearTimeout')

  assert.equal(await invokeWithTimeout<string>('ping', undefined, undefined, false), 'done')
  assert.equal(cleared.mock.callCount(), 1)
})
