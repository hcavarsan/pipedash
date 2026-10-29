import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

import type { ProviderSummary } from '../../types'

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

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation, useNavigate, useSearchParams } = await import('react-router-dom')
const { AppLayout } = await import('./AppLayout')
const { service } = await import('../../services')
const view = () => within(document.body)
const clients = new Set<InstanceType<typeof QueryClient>>()

const newClient = () => {
  const created = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

  clients.add(created)

  return created
}

beforeEach(() => {
  notifyManager.setNotifyFunction((callback) => act(callback))
})

afterEach(async () => {
  await act(async () => cleanup())
  notifyManager.setNotifyFunction((callback) => callback())
  for (const created of clients) {
    created.clear()
  }
  clients.clear()
  window.innerWidth = 1400
})

const stubShell = (context: TestContext, listProviders: () => Promise<ProviderSummary[]>) => {
  context.mock.method(service, 'listProviders', listProviders)
  context.mock.method(service, 'getAvailablePlugins', async () => [])
  context.mock.method(service, 'getVaultStatus', async () => ({
    is_unlocked: true,
    password_source: 'none',
    backend: 'sqlite',
    requires_password: false,
  }))
}

const renderLayout = () =>
  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client: newClient() },
        createElement(MemoryRouter, { initialEntries: ['/pipelines'] }, createElement(SearchPage))
      )
    )
  )

after(async () => {
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
  const user = userEvent.setup({ document })
  stubShell(context, async () => [])
  renderLayout()

  const heading = view().getByRole('heading', { name: 'Pipelines' })
  await waitFor(() => assert.ok(document.activeElement === heading, 'Initial navigation focuses the heading'))
  const input = view().getByRole('textbox', { name: 'Search pipelines' })
  assert.ok(input instanceof HTMLInputElement)
  input.focus()

  let typed = ''
  for (const chunk of ['b', 'u', 'ild']) {
    typed += chunk
    const value = typed
    await user.keyboard(chunk)
    await act(async () => {
      const { promise, resolve } = Promise.withResolvers<void>()
      requestAnimationFrame(() => resolve())
      await promise
    })
    assert.equal(input.value, value)
    assert.ok(document.activeElement === input, 'Search updates must not move focus away from the input')
  }

  await user.click(view().getByRole('button', { name: 'Show pinned' }))
  await waitFor(() => assert.ok(document.activeElement === heading, 'Changing the view focuses the heading'))
  input.focus()
  await user.click(view().getByRole('button', { name: 'Open settings' }))
  await waitFor(() => {
    assert.ok(
      document.activeElement === view().getByRole('heading', { name: 'Settings' }),
      'Navigation focuses the new heading'
    )
  })
})

test('the sidebar shows a loading status while providers load', async (context) => {
  stubShell(context, () => Promise.withResolvers<ProviderSummary[]>().promise)
  renderLayout()

  assert.ok(await view().findByText('Loading providers'))
  assert.equal(view().queryByText('No providers yet'), null)
})

test('the sidebar reports a failed provider query and offers a retry', async (context) => {
  stubShell(context, () => Promise.reject(new Error('providers unavailable')))
  renderLayout()

  assert.ok(await view().findByText("Couldn't load providers"))
  assert.ok(view().getByText('providers unavailable'))
  assert.ok(view().getByRole('button', { name: 'Retry' }))
  assert.equal(view().queryByText('Loading providers'), null)
})

test('the sidebar explains that no providers exist yet', async (context) => {
  stubShell(context, async () => [])
  renderLayout()

  assert.ok(await view().findByText('No providers yet'))
  assert.equal(view().queryByText("Couldn't load providers"), null)
})

test('navigating on a narrow screen closes the open sidebar', async (context) => {
  stubShell(context, async () => [])
  window.innerWidth = 500
  const user = userEvent.setup({ document })

  renderLayout()

  await user.click(await view().findByRole('button', { name: 'Expand sidebar' }))
  assert.ok(view().getByRole('button', { name: 'Collapse sidebar' }))

  await user.click(view().getByRole('button', { name: 'Open settings' }))

  await waitFor(() => assert.ok(view().getByRole('button', { name: 'Expand sidebar' })))
  assert.equal(view().queryByRole('button', { name: 'Collapse sidebar' }), null)
})
