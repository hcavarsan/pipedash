import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
mockWindows('main')
mockIPC(
  (command) => {
    switch (command) {
      case 'list_providers':
      case 'get_available_plugins':
        return []
      case 'get_vault_status':
        return { is_initialized: true, is_unlocked: true, requires_password: false }
      case 'plugin:window|is_focused':
        return true
      case 'plugin:window|is_maximized':
        return false
      case 'set_refresh_mode':
        return
      default:
        throw new Error(`Unexpected command: ${command}`)
    }
  },
  { shouldMockEvents: true }
)

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation, useNavigate, useSearchParams } = await import('react-router-dom')
const { AppLayout } = await import('./AppLayout')
const { apiService, tauriService } = await import('../../services')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

function SearchPage() {
  const [params, setParams] = useSearchParams()
  const location = useLocation()
  const navigate = useNavigate()

  return createElement(
    AppLayout,
    null,
    createElement(
      'h1',
      { 'data-page-heading': '', tabIndex: -1 },
      location.pathname === '/pipelines' ? 'Pipelines' : 'Settings'
    ),
    createElement('input', {
      'aria-label': 'Search pipelines',
      value: params.get('search') ?? '',
      onChange: (event) => setParams({ search: event.currentTarget.value }),
    }),
    createElement('button', { type: 'button', onClick: () => navigate('/settings') }, 'Open settings'),
    createElement('button', { type: 'button', onClick: () => setParams({ view: 'pinned' }) }, 'Show pinned')
  )
}

test('URL-backed searches keep input focus, while page and view changes focus the heading', async (context) => {
  context.mock.method(apiService, 'listProviders', tauriService.listProviders)
  context.mock.method(apiService, 'getAvailablePlugins', tauriService.getAvailablePlugins)
  context.mock.method(apiService, 'getVaultStatus', tauriService.getVaultStatus)
  notifyManager.setNotifyFunction((callback) => act(callback))
  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client },
        createElement(MemoryRouter, { initialEntries: ['/pipelines'] }, createElement(SearchPage))
      )
    )
  )

  const heading = view().getByRole('heading', { name: 'Pipelines' })
  await waitFor(() => assert.ok(document.activeElement === heading, 'Initial navigation focuses the heading'))
  const input = view().getByRole('textbox', { name: 'Search pipelines' })
  assert.ok(input instanceof HTMLInputElement)
  input.focus()

  for (const value of ['b', 'bu', 'build']) {
    fireEvent.change(input, { target: { value } })
    await act(async () => {
      const { promise, resolve } = Promise.withResolvers<void>()
      requestAnimationFrame(() => resolve())
      await promise
    })
    assert.equal(input.value, value)
    assert.ok(document.activeElement === input, 'Search updates must not move focus away from the input')
  }

  fireEvent.click(view().getByRole('button', { name: 'Show pinned' }))
  await waitFor(() => assert.ok(document.activeElement === heading, 'Changing the view focuses the heading'))
  input.focus()
  fireEvent.click(view().getByRole('button', { name: 'Open settings' }))
  await waitFor(() => {
    assert.ok(
      document.activeElement === view().getByRole('heading', { name: 'Settings' }),
      'Navigation focuses the new heading'
    )
  })
})
