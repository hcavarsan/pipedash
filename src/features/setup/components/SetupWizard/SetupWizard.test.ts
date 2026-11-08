import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { isRecord } from '../../../../utils/isRecord'

GlobalRegistrator.register({ width: 1400, height: 900 })

const backend: { storageFails: boolean; calls: string[] } = { storageFails: true, calls: [] }

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args?: unknown) => {
      backend.calls.push(command)
      switch (command) {
        case 'get_storage_config':
          if (backend.storageFails) {
            throw new Error('storage unavailable')
          }

          return null
        case 'get_vault_password_status':
          return { is_set: false, env_var_name: 'PIPEDASH_VAULT_PASSWORD' }
        case 'get_default_data_dir':
          return '/home/user/.pipedash'
        case 'prepare_setup_draft': {
          const draft = isRecord(args) && isRecord(args.draft) ? args.draft : {}
          const errors: Record<string, string> = {}

          if (String(draft.vault_password ?? '').length < 8) {
            errors.vault_password = 'Use at least 8 characters'
          }
          if (draft.vault_password !== draft.vault_password_confirm) {
            errors.vault_password_confirm = 'Passwords do not match'
          }

          return { config: {}, errors, password_warnings: [], needs_migration: false }
        }
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { MemoryRouter } = await import('react-router-dom')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { SetupWizard } = await import('./SetupWizard')
const view = () => within(document.body)

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  backend.storageFails = true
  backend.calls = []
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

function renderWizard() {
  return render(
    createElement(
      MantineProvider,
      null,
      createElement(
        MemoryRouter,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(SetupWizard, { opened: true, onComplete: () => undefined, onClose: () => undefined })
        )
      )
    )
  )
}

test('a failed storage read shows an error with retry and never offers the first-run form', async () => {
  renderWizard()

  await view().findByText("Couldn't load storage configuration")
  assert.equal(view().queryByRole('button', { name: 'Continue' }), null)
  assert.equal(view().queryByRole('radio', { name: /SQLite/ }), null)
})

test('retrying after a failed read opens the storage form', async () => {
  const user = userEvent.setup({ document })

  renderWizard()
  await view().findByText("Couldn't load storage configuration")

  backend.storageFails = false
  await user.click(view().getByRole('button', { name: /retry|try again/i }))

  await waitFor(() => assert.ok(view().getByRole('button', { name: 'Continue' })))
  assert.ok(view().getByRole('button', { name: 'Continue' }))
})

test('continue stays disabled until the vault password is valid', async () => {
  const user = userEvent.setup({ document })

  backend.storageFails = false
  renderWizard()

  const next = await view().findByRole('button', { name: 'Continue' })
  assert.ok(next instanceof HTMLButtonElement)
  assert.equal(next.disabled, true)

  await user.type(view().getByLabelText(/^Password/), 'longenough1')
  await user.type(view().getByLabelText(/^Confirm password/), 'longenough1')
  await waitFor(() => assert.equal(next.disabled, false))
})
