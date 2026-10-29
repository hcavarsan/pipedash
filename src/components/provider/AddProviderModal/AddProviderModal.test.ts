import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { PluginMetadata, ProviderConfig } from '../../../types'

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

  Object.assign(window, {
    __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
    __TAURI_INTERNALS__: {
      invoke: async (command: string) => {
        if (command === 'get_available_plugins') {
          return plugins
        }
        throw new Error(`Unexpected command: ${command}`)
      },
    },
  })

  const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
  const { userEvent } = await import('@testing-library/user-event')
  const { MantineProvider } = await import('@mantine/core')
  const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
  const { AddProviderModal } = await import('./index')
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

    await user.click(await view().findByRole('combobox', { name: /Provider type/ }))
    await user.click(await view().findByRole('option', { name: 'GitHub', hidden: true }))
    await user.click(view().getByRole('button', { name: /^Next/ }))

    assert.ok((await view().findAllByText('Token is required')).length > 0)
    assert.equal(closed.value, false)
  })
}
