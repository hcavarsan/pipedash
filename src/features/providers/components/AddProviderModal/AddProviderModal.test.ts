import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { UserEvent } from '@testing-library/user-event'
import { createElement } from 'react'

import type { PluginMetadata } from '../../../../types/plugins'
import type { ProviderDraft, ProviderDraftResult } from '../../../../types/provider_draft'
import type { ProviderConfig } from '../../../../types/providers'

import { configFieldFixture, pluginFixture } from './pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const plugins: PluginMetadata[] = [
  pluginFixture({
    required_permissions: [],
    config_schema: {
      fields: [
        configFieldFixture({ key: 'token', label: 'Token', field_type: 'Password', required: true }),
        configFieldFixture({ key: 'note', label: 'Note', field_type: 'Text', required: false }),
        configFieldFixture({
          key: 'status_page_url',
          label: 'Status page URL',
          field_type: 'Text',
          required: false,
          validation_regex: '^https://',
          validation_message: 'Must be an https URL',
          section: 'settings',
        }),
      ],
    },
  }),
]

const secondPage: { waiting: (() => void)[]; requests: number } = { waiting: [], requests: 0 }
const saves: { handler: (draft: ProviderDraft) => Promise<void> } = { handler: async () => undefined }
const selectionPolicy = { required: true, empty: false }
const validation: { hold: Promise<void> | null; held: number; delivered: number } = {
  hold: null,
  held: 0,
  delivered: 0,
}

function validateDraft(draft: ProviderDraft): ProviderDraftResult {
  const errors: Record<string, string> = {}
  const missing: string[] = []
  const fields = plugins.find((plugin) => plugin.provider_type === draft.provider_type)?.config_schema.fields ?? []

  if (!draft.name.trim()) {
    errors.providerName = 'Provider name is required'
  }
  for (const field of fields) {
    const value = draft.values[field.key] ?? ''

    if (field.required && !value.trim()) {
      errors[field.key] = `${field.label} is required`
      missing.push(field.label)
    } else if (field.validation_regex && value && !new RegExp(field.validation_regex).test(value)) {
      errors[field.key] = field.validation_message ?? `Invalid format for ${field.label}`
    }
  }

  return {
    errors,
    active_field_keys: fields.map((field) => field.key),
    missing_field_labels: missing,
    credentials_complete: missing.length === 0,
    selection_needs_refresh: false,
    selection_required: selectionPolicy.required,
  }
}
const job = (id: string) => ({ id, name: id, description: null, organization: 'org', repository: 'org/repo' })
const page = (ids: string[], number: number, hasMore: boolean) => ({
  items: ids.map(job),
  page: number,
  page_size: 2,
  total_count: 4,
  total_pages: 2,
  has_more: hasMore,
})

function draftArg(args: { draft?: ProviderDraft }): ProviderDraft {
  if (!args.draft) {
    throw new Error('Missing draft')
  }

  return args.draft
}

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args: { draft?: ProviderDraft; page?: number } = {}) => {
      if (command === 'get_available_plugins') {
        return plugins
      }
      if (command === 'fetch_provider_organizations') {
        return [{ id: 'org', name: 'org', description: null }]
      }
      if (command === 'validate_provider_draft') {
        const result = validateDraft(draftArg(args))
        const hold = validation.hold

        if (hold !== null) {
          validation.held += 1
          await hold
          validation.delivered += 1
        }

        return result
      }
      if (command === 'save_provider_draft') {
        await saves.handler(draftArg(args))
        return 7
      }
      if (command === 'get_matching_pipeline_ids') {
        secondPage.requests += 1
        const { promise, resolve } = Promise.withResolvers<void>()
        secondPage.waiting.push(resolve)
        await promise
        return ['a', 'b', 'c', 'd']
      }
      if (command === 'preview_provider_pipelines') {
        if (selectionPolicy.empty) {
          return { ...page([], 1, false), total_count: 0, total_pages: 0 }
        }
        return args?.page === 2 ? page(['c', 'd'], 2, false) : page(['a', 'b'], 1, true)
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
  name: 'Stub',
  provider_type: 'stub',
  token: 'secret',
  config: { note: 'old' },
  refresh_interval: 30,
}

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  selectionPolicy.required = true
  validation.hold = null
  validation.held = 0
  validation.delivered = 0
  selectionPolicy.empty = false
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

function renderEdit(onSave: (draft: ProviderDraft) => Promise<void>) {
  const closed = { value: false }
  saves.handler = onSave

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
  const saved: ProviderDraft[] = []
  const closed = renderEdit(async (draft) => {
    saved.push(draft)
    await gate.promise
  })

  await user.click(await view().findByRole('button', { name: /^Continue/ }))
  const note = await view().findByRole('textbox', { name: 'Note' })
  assert.ok(note instanceof HTMLInputElement)
  await waitFor(() => assert.equal(note.disabled, false))
  await user.clear(note)
  await user.type(note, 'new')
  await user.click(view().getByRole('button', { name: 'Save' }))

  await waitFor(() => assert.equal(saved.length, 1))
  assert.equal(note.disabled, true)
  assert.equal(view().getByRole('button', { name: 'Back' }).hasAttribute('disabled'), true)
  assert.equal(
    view()
      .getByRole('button', { name: /^Continue/ })
      .hasAttribute('disabled'),
    true
  )
  assert.equal(closed.value, false)

  await act(async () => gate.resolve())
  await waitFor(() => assert.equal(closed.value, true))
  assert.equal(saved[0]?.values.note, 'new')
  assert.equal(saved[0]?.provider_id, 7)
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

  await user.click(await view().findByRole('button', { name: /^Continue/ }))
  const note = await view().findByRole('textbox', { name: 'Note' })
  assert.ok(note instanceof HTMLInputElement)
  await waitFor(() => assert.equal(note.disabled, false))
  await user.clear(note)
  await user.type(note, 'kept')
  await user.click(view().getByRole('button', { name: 'Save' }))

  await view().findByText(/provider rejected the update/)
  assert.equal(closed.value, false)
  assert.equal(note.value, 'kept')
  assert.equal(note.disabled, false)

  await user.click(view().getByRole('button', { name: 'Save' }))
  await waitFor(() => assert.equal(closed.value, true))
  assert.equal(attempts, 2)
})

test('the save error disappears when the user edits a field', async () => {
  const user = userEvent.setup({ document })
  renderEdit(async () => {
    throw new Error('provider rejected the update')
  })

  await user.click(await view().findByRole('button', { name: /^Continue/ }))
  const note = await view().findByRole('textbox', { name: 'Note' })
  await waitFor(() => assert.equal(note.hasAttribute('disabled'), false))
  await user.click(view().getByRole('button', { name: 'Save' }))
  await view().findByText(/provider rejected the update/)

  await user.type(note, '!')
  await waitFor(() => assert.ok(!view().queryByText(/provider rejected the update/)))
})

function renderAdd(onSave: (draft: ProviderDraft) => Promise<void>): { value: boolean } {
  const closed = { value: false }
  saves.handler = onSave
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
        })
      )
    )
  )

  return closed
}

test('the provider step checks only its own fields, then a required credential blocks the next step', async () => {
  const user = userEvent.setup({ document })
  renderAdd(async () => undefined)

  await user.click(await view().findByRole('button', { name: 'Stub' }))
  await user.click(view().getByRole('button', { name: 'Advanced settings' }))
  await user.type(view().getByRole('textbox', { name: 'Status page URL' }), 'http://status.example.com')
  await user.click(view().getByRole('button', { name: 'Advanced settings' }))
  await user.click(view().getByRole('button', { name: /^Continue/ }))

  assert.ok((await view().findAllByText('Must be an https URL')).length > 0)
  assert.equal(view().getByRole('button', { name: 'Advanced settings' }).getAttribute('aria-expanded'), 'true')
  assert.equal(view().queryByText('Token is required'), null)

  const statusPage = await waitFor(() => {
    const field = view().getByRole('textbox', { name: 'Status page URL' })
    assert.equal(field.hasAttribute('disabled'), false)
    return field
  })
  await user.clear(statusPage)
  await user.type(statusPage, 'https://status.example.com')
  await user.click(view().getByRole('button', { name: /^Continue/ }))

  await view().findByLabelText(/^Token/)
  assert.equal(view().queryByText('Token is required'), null)

  await user.click(view().getByRole('button', { name: /^Continue/ }))
  assert.ok((await view().findAllByText('Token is required')).length > 0)
})

test('a validation response for a value the user already replaced does not set an error', async () => {
  const user = userEvent.setup({ document })
  renderAdd(async () => undefined)

  await user.click(await view().findByRole('button', { name: 'Stub' }))
  await user.click(view().getByRole('button', { name: 'Advanced settings' }))
  const statusPage = await view().findByRole('textbox', { name: 'Status page URL' })
  await waitFor(() => assert.equal(statusPage.hasAttribute('disabled'), false))
  await user.type(statusPage, 'http://status.example.com')

  const gate = Promise.withResolvers<void>()
  validation.hold = gate.promise
  await user.tab()
  await waitFor(() => assert.ok(validation.held > 0))

  await user.clear(statusPage)
  await user.type(statusPage, 'https://status.example.com')
  validation.hold = null
  await act(async () => gate.resolve())
  await waitFor(() => assert.equal(validation.delivered, validation.held))
  await act(async () => undefined)

  assert.equal(view().queryByText('Must be an https URL'), null)
})

test('an empty provider saves when monitoring is automatic', async () => {
  const user = userEvent.setup({ document })
  const added: ProviderDraft[] = []

  selectionPolicy.required = false
  selectionPolicy.empty = true
  const closed = renderAdd(async (draft) => {
    added.push(draft)
  })

  await user.click(await view().findByRole('button', { name: 'Stub' }))
  await user.click(view().getByRole('button', { name: /^Continue/ }))
  const token = await view().findByLabelText(/^Token/)
  assert.ok(token instanceof HTMLInputElement)
  await waitFor(() => assert.equal(token.disabled, false))
  await user.type(token, 'secret')
  await user.click(view().getByRole('button', { name: /^Continue/ }))
  const add = await view().findByRole('button', { name: 'Add provider' })

  await waitFor(() => assert.equal(add.hasAttribute('disabled'), false))
  await user.click(add)
  await waitFor(() => assert.equal(closed.value, true))
  assert.deepEqual(added[0]?.selected_pipeline_ids, [])
})

async function withVisibleRows(run: () => Promise<void>): Promise<void> {
  const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 600 })

  try {
    await run()
  } finally {
    if (clientHeight) {
      Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, 'clientHeight')
    }
  }
}

async function selectFirstPipeline(user: UserEvent): Promise<void> {
  await user.click(await view().findByRole('button', { name: 'Stub' }))
  await user.click(view().getByRole('button', { name: /^Continue/ }))
  await user.type(await view().findByLabelText(/^Token/), 'secret')
  await user.click(view().getByRole('button', { name: /^Continue/ }))
  await user.click(await view().findByRole('checkbox', { name: 'a' }))
}

test('the pipeline selection stays locked while the provider saves', () =>
  withVisibleRows(async () => {
    const user = userEvent.setup({ document })
    const gate = Promise.withResolvers<void>()
    const added: ProviderDraft[] = []
    const closed = renderAdd(async (draft) => {
      added.push(draft)
      await gate.promise
    })

    await selectFirstPipeline(user)
    await user.click(view().getByRole('button', { name: 'Add provider' }))
    await waitFor(() => assert.equal(added.length, 1))

    secondPage.requests = 0
    await user.click(view().getByRole('checkbox', { name: 'b' }))
    await user.click(view().getByRole('checkbox', { name: 'Select all' }))
    assert.equal(view().getByRole('checkbox', { name: 'b' }).matches(':checked'), false)
    assert.equal(secondPage.requests, 0)

    await act(async () => gate.resolve())
    await waitFor(() => assert.equal(closed.value, true))
    assert.deepEqual(added[0]?.selected_pipeline_ids, ['a'])
  }))

test('the provider cannot be added while select all is still loading', () =>
  withVisibleRows(async () => {
    const user = userEvent.setup({ document })
    const added: ProviderDraft[] = []
    secondPage.requests = 0

    renderAdd(async (draft) => {
      added.push(draft)
    })

    await selectFirstPipeline(user)
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
    assert.deepEqual(added[0]?.selected_pipeline_ids?.toSorted(), ['a', 'b', 'c', 'd'])
  }))
