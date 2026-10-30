import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { PluginMetadata, ProviderConfig } from '../../../../types/domain'

import { configFieldFixture, pluginFixture } from './pluginFixture'

if (process.env.PIPEDASH_PROVIDER_WIZARD_TEST !== '1') {
  test('provider wizard behavior runs in its own DOM process', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_PROVIDER_WIZARD_TEST: '1' },
      encoding: 'utf8',
      timeout: 60000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 1400, height: 900 })

  const plugins: PluginMetadata[] = [
    pluginFixture({
      required_permissions: [],
      config_schema: {
        fields: [
          configFieldFixture({ key: 'token', label: 'Token', field_type: 'Password', required: true }),
          configFieldFixture({ key: 'note', label: 'Note', field_type: 'Text', required: false }),
        ],
      },
    }),
  ]

  const secondPage: { waiting: (() => void)[]; requests: number } = { waiting: [], requests: 0 }
  const job = (id: string) => ({ id, name: id, description: null, organization: 'org', repository: 'org/repo' })
  const page = (ids: string[], number: number, hasMore: boolean) => ({
    items: ids.map(job),
    page: number,
    page_size: 2,
    total_count: 4,
    total_pages: 2,
    has_more: hasMore,
  })

  Object.assign(window, {
    __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
    __TAURI_INTERNALS__: {
      invoke: async (command: string, args?: { page?: number }) => {
        if (command === 'get_available_plugins') {
          return plugins
        }
        if (command === 'fetch_provider_organizations') {
          return [{ id: 'org', name: 'org', description: null }]
        }
        if (command === 'preview_provider_pipelines') {
          if (args?.page !== 2) {
            return page(['a', 'b'], 1, true)
          }
          secondPage.requests += 1
          const { promise, resolve } = Promise.withResolvers<void>()
          secondPage.waiting.push(resolve)
          await promise
          return page(['c', 'd'], 2, false)
        }
        throw new Error(`Unexpected command: ${command}`)
      },
    },
  })

  const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
  const { userEvent } = await import('@testing-library/user-event')
  const { MantineProvider } = await import('@mantine/core')
  const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
  const { AddProviderModal } = await import('./AddProviderModal')
  const view = () => within(document.body)

  const existing: ProviderConfig & { id: number } = {
    id: 7,
    name: 'GitHub',
    provider_type: 'github',
    token: 'secret',
    config: { note: 'old' },
    refresh_interval: 30,
  }

  let client = new QueryClient()

  beforeEach(() => {
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    notifyManager.setNotifyFunction((callback) => act(callback))
  })

  afterEach(async () => {
    await act(async () => cleanup())
    client.clear()
    mock.restoreAll()
  })

  after(async () => {
    notifyManager.setNotifyFunction((callback) => callback())
    await GlobalRegistrator.unregister()
  })

  function renderEdit(onUpdate: (id: number, config: ProviderConfig) => Promise<void>) {
    const closed = { value: false }

    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(AddProviderModal, {
            opened: true,
            onClose: () => {
              closed.value = true
            },
            editMode: true,
            onUpdate,
            existingProvider: existing,
          })
        )
      )
    )

    return closed
  }

  test('credential fields and navigation stay locked while the save is pending', async () => {
    const user = userEvent.setup({ document })
    const gate = Promise.withResolvers<void>()
    const saved: ProviderConfig[] = []
    const closed = renderEdit(async (_id, config) => {
      saved.push(config)
      await gate.promise
    })

    const note = await view().findByRole('textbox', { name: 'Note' })
    await user.clear(note)
    await user.type(note, 'new')
    await user.click(view().getByRole('button', { name: 'Save' }))

    await waitFor(() => assert.equal(saved.length, 1))
    assert.ok(note instanceof HTMLInputElement)
    assert.equal(note.disabled, true)
    assert.equal(view().getByRole('textbox', { name: 'Provider name' }).hasAttribute('disabled'), true)
    assert.equal(view().getByRole('button', { name: /^Next/ }).hasAttribute('disabled'), true)
    assert.equal(closed.value, false)

    await act(async () => gate.resolve())
    await waitFor(() => assert.equal(closed.value, true))
    assert.equal(saved[0]?.config.note, 'new')
  })

  test('a rejected save keeps the dialog open with the typed values and allows a retry', async () => {
    const user = userEvent.setup({ document })
    let attempts = 0
    const closed = renderEdit(async () => {
      attempts += 1
      if (attempts === 1) {
        throw new Error('provider rejected the update')
      }
    })

    const note = await view().findByRole('textbox', { name: 'Note' })
    await user.clear(note)
    await user.type(note, 'kept')
    await user.click(view().getByRole('button', { name: 'Save' }))

    await view().findByText(/provider rejected the update/)
    assert.equal(closed.value, false)
    assert.ok(note instanceof HTMLInputElement)
    assert.equal(note.value, 'kept')
    assert.equal(note.disabled, false)

    await user.click(view().getByRole('button', { name: 'Save' }))
    await waitFor(() => assert.equal(closed.value, true))
    assert.equal(attempts, 2)
  })

  test('a required credential blocks the continue step and reports the field', async () => {
    const user = userEvent.setup({ document })
    const closed = { value: false }

    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(AddProviderModal, {
            opened: true,
            onClose: () => {
              closed.value = true
            },
            onAdd: async () => undefined,
          })
        )
      )
    )

    await user.click(await view().findByRole('button', { name: 'GitHub' }))
    await user.click(view().getByRole('button', { name: /^Next/ }))

    assert.ok((await view().findAllByText('Token is required')).length > 0)
    assert.equal(closed.value, false)
  })

  test('the provider cannot be added while select all is still loading pages', async () => {
    const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 600 })

    try {
      const user = userEvent.setup({ document })
      const added: ProviderConfig[] = []
      secondPage.requests = 0

      render(
        createElement(
          MantineProvider,
          null,
          createElement(
            QueryClientProvider,
            { client },
            createElement(AddProviderModal, {
              opened: true,
              onClose: () => undefined,
              onAdd: async (config) => {
                added.push(config)
              },
            })
          )
        )
      )

      await user.click(await view().findByRole('button', { name: 'GitHub' }))
      await user.type(await view().findByLabelText(/^Token/), 'secret')
      await user.click(view().getByRole('button', { name: /^Next/ }))
      await user.click(await view().findByRole('checkbox', { name: 'a' }))
      await user.click(view().getByRole('checkbox', { name: 'Select all' }))
      await waitFor(() => assert.ok(secondPage.requests > 0))

      assert.equal(view().getByRole('button', { name: 'Add provider' }).hasAttribute('disabled'), true)

      await act(async () => {
        for (const release of secondPage.waiting.splice(0)) {
          release()
        }
      })
      await view().findByText('4 selected')
      await user.click(view().getByRole('button', { name: 'Add provider' }))

      await waitFor(() => assert.equal(added.length, 1))
      assert.deepEqual(added[0]?.config.selected_items?.split(',').sort(), ['a', 'b', 'c', 'd'])
    } finally {
      if (clientHeight) {
        Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight)
      } else {
        Reflect.deleteProperty(HTMLElement.prototype, 'clientHeight')
      }
    }
  })
}
