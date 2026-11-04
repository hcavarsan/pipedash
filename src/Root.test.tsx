import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { PendingNativeIntent } from './types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

let pending: PendingNativeIntent | null = null
const acknowledged: number[] = []
let handler = 0

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: () => ++handler,
    invoke: async (command: string, args: Record<string, unknown> = {}) => {
      if (command === 'plugin:event|listen') {
        return args.handler
      }
      if (command === 'get_connection') {
        return { mode: 'unset' }
      }
      if (command === 'native_peek_pending_intent') {
        return pending
      }
      if (command === 'native_ack_pending_intent') {
        acknowledged.push(Number(args.id))
        pending = null
      }
      return undefined
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { loadConnection } = await import('./services/connection')
const { Root } = await import('./Root')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

afterEach(async () => {
  await act(async () => cleanup())
  pending = null
  acknowledged.length = 0
})

test('a pairing link reaches the connect form before the server is set up', async () => {
  await loadConnection()
  pending = { id: 21, intent: { type: 'pair', link: 'pipedash://pair?code=ABCD' } }
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter initialEntries={['/']}>
          <Root />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await within(document.body).findByDisplayValue('pipedash://pair?code=ABCD')
  await waitFor(() => assert.deepEqual(acknowledged, [21]))
})
