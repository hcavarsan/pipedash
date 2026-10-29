import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const backend: { configErrors: string[]; calls: string[] } = { configErrors: [], calls: [] }

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string) => {
      backend.calls.push(command)
      switch (command) {
        case 'check_setup_status':
          return {
            config_exists: true,
            config_valid: backend.configErrors.length === 0,
            validation_errors: backend.configErrors,
            needs_setup: false,
            needs_migration: false,
          }
        case 'get_storage_paths':
          return {
            config_file: '/home/user/.local/share/pipedash/config.toml',
            pipedash_db: '/home/user/.local/share/pipedash/pipedash.db',
            metrics_db: '/home/user/.local/share/pipedash/metrics.db',
          }
        case 'restart_app':
          return Promise.withResolvers<null>().promise
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { MemoryRouter } = await import('react-router-dom')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { App } = await import('./App')
const view = () => within(document.body)

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  backend.configErrors = ['Failed to parse config: unclosed table']
  backend.calls = []
  notifyManager.setNotifyFunction((callback) => act(callback))
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const renderApp = async (): Promise<void> => {
  await act(async () => {
    render(
      <MantineProvider>
        <MemoryRouter>
          <QueryClientProvider client={client}>
            <App />
          </QueryClientProvider>
        </MemoryRouter>
      </MantineProvider>
    )
  })
  await waitFor(() => assert.ok(view().getByText('Configuration file has errors')))
}

test('retry after fixing a broken config restarts the app instead of opening it without a core', async () => {
  const user = userEvent.setup()
  await renderApp()

  backend.configErrors = []
  await user.click(view().getByRole('button', { name: 'Retry' }))

  await waitFor(() => assert.ok(backend.calls.includes('restart_app')))
  assert.ok(view().getByText('Configuration file has errors'))
})

test('retry on a config that is still broken shows the new errors and does not restart', async () => {
  const user = userEvent.setup()
  await renderApp()

  backend.configErrors = ['storage.backend: unknown variant']
  await user.click(view().getByRole('button', { name: 'Retry' }))

  await waitFor(() => assert.ok(view().getByText('storage.backend: unknown variant')))
  assert.equal(backend.calls.includes('restart_app'), false)
})
