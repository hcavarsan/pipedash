import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { UpdateStatus } from '../../../types/native'

GlobalRegistrator.register({ width: 1400, height: 900 })
const EVENT = 'update-status-changed'
const idle: UpdateStatus = {
  phase: 'idle',
  current_version: '0.13.0',
  version: null,
  notes: null,
  downloaded: 0,
  total: null,
  message: '',
}
const available: UpdateStatus = {
  ...idle,
  phase: 'available',
  version: '0.14.0',
  notes: 'Faster refresh',
  message: 'Pipedash 0.14.0 is available.',
}
const callbacks = new Map<number, (event: unknown) => void>()
const listeners = new Map<string, number>()
const commands: string[] = []
let handler = 0

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: (callback: (event: unknown) => void) => {
      callbacks.set(++handler, callback)
      return handler
    },
    invoke: async (command: string, args: { event?: string; handler?: number } = {}) => {
      if (command === 'plugin:event|listen') {
        listeners.set(String(args.event), Number(args.handler))
        return args.handler
      }
      if (command === 'plugin:event|unlisten') {
        return
      }
      commands.push(command)
      if (command === 'get_update_status') {
        return idle
      }
      if (command === 'install_update') {
        return null
      }
      throw new Error(`Unexpected native command: ${command}`)
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { useUpdaterLifecycle } = await import('../queries/useUpdaterQueries')
const { UpdatePrompt } = await import('./UpdatePrompt')
const view = () => within(document.body)

function Screen() {
  useUpdaterLifecycle()
  return <UpdatePrompt />
}

async function mountWith(status: UpdateStatus) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <Screen />
      </QueryClientProvider>
    </MantineProvider>
  )
  await waitFor(() => assert.ok(commands.includes('get_update_status')))
  await publish(status)
}

async function publish(payload: UpdateStatus) {
  await act(async () => callbacks.get(listeners.get(EVENT) ?? 0)?.({ event: EVENT, id: 1, payload }))
}

const installs = () => commands.filter((command) => command === 'install_update').length

afterEach(async () => {
  await act(async () => cleanup())
  commands.length = 0
  callbacks.clear()
  listeners.clear()
})

after(() => GlobalRegistrator.unregister())

test('a found update only asks, and Later keeps that version dismissed', async () => {
  const user = userEvent.setup({ document, delay: null })
  await mountWith(available)
  assert.ok(await view().findByText('Pipedash 0.14.0 is available.'))
  assert.ok(view().getByText('Faster refresh'))
  assert.equal(installs(), 0)

  await user.click(view().getByRole('button', { name: 'Later' }))
  await waitFor(() => assert.equal(view().queryByText('Pipedash 0.14.0 is available.'), null))
  await publish(available)
  assert.equal(view().queryByText('Pipedash 0.14.0 is available.'), null)
  assert.equal(installs(), 0)

  await publish({ ...available, version: '0.15.0', message: 'Pipedash 0.15.0 is available.' })
  assert.ok(await view().findByText('Pipedash 0.15.0 is available.'))
})

test('Update and restart installs once and the prompt stays until the restart', async () => {
  const user = userEvent.setup({ document, delay: null })
  await mountWith(available)
  await user.click(await view().findByRole('button', { name: 'Update and restart' }))
  await waitFor(() => assert.equal(installs(), 1))

  await publish({
    ...available,
    phase: 'downloading',
    total: 200,
    downloaded: 50,
    message: 'Downloading Pipedash 0.14.0…',
  })
  assert.ok(await view().findByText('Downloading Pipedash 0.14.0…'))
  const later = view().getByRole('button', { name: 'Later' })
  assert.ok(later instanceof HTMLButtonElement && later.disabled)
  assert.equal(view().getByRole('progressbar', { name: 'Download progress' }).getAttribute('aria-valuenow'), '25')
  assert.equal(installs(), 1)
})
