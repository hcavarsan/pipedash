import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { UserEvent } from '@testing-library/user-event'
import { createElement } from 'react'

import { isRecord } from '../../../../utils/isRecord'

GlobalRegistrator.register({ width: 1400, height: 900 })

const backend: {
  storageFails: boolean
  prepareFails: boolean
  applyFails: boolean
  prepareGate: Promise<void>
  calls: string[]
} = { storageFails: true, prepareFails: false, applyFails: false, prepareGate: Promise.resolve(), calls: [] }

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
          await backend.prepareGate

          if (backend.prepareFails) {
            throw new Error('storage check unavailable')
          }

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
        case 'apply_setup_draft':
          if (backend.applyFails) {
            throw new Error('disk full')
          }

          return { saved: true, migration_stats: null, restart_required: false }
        case 'bootstrap_app':
          return undefined
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { MemoryRouter } = await import('react-router-dom')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { SetupWizard } = await import('./SetupWizard')
const view = () => within(document.body)

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  backend.storageFails = true
  backend.prepareFails = false
  backend.applyFails = false
  backend.prepareGate = Promise.resolve()
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

interface WizardCallbacks {
  onComplete?: () => void
  onClose?: () => void
}

function renderWizard({ onComplete = () => undefined, onClose = () => undefined }: WizardCallbacks = {}) {
  return render(
    createElement(
      MantineProvider,
      null,
      createElement(
        ModalsProvider,
        null,
        createElement(
          MemoryRouter,
          null,
          createElement(
            QueryClientProvider,
            { client },
            createElement(SetupWizard, { opened: true, onComplete, onClose })
          )
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

const passwordField = () => view().getByLabelText(/^Password/)
const confirmField = () => view().getByLabelText(/^Confirm password/)
const prepareCalls = (): number => backend.calls.filter((command) => command === 'prepare_setup_draft').length

function continueButton(): HTMLButtonElement {
  const button = view().getByRole('button', { name: 'Continue' })
  assert.ok(button instanceof HTMLButtonElement)

  return button
}

async function typeBoth(user: UserEvent, text: string): Promise<void> {
  await user.type(passwordField(), text)
  await user.type(confirmField(), text)
}

async function continueWhenReady(user: UserEvent): Promise<void> {
  await waitFor(() => assert.equal(continueButton().disabled, false))
  await user.click(continueButton())
}

test('a rejected storage check shows an error with retry and a retry recovers', async () => {
  const user = userEvent.setup({ document })

  backend.storageFails = false
  backend.prepareFails = true
  renderWizard()

  await view().findByText("Couldn't check the storage settings")
  assert.equal(continueButton().disabled, true)

  backend.prepareFails = false
  await user.click(view().getByRole('button', { name: 'Retry' }))
  await waitFor(() => assert.equal(view().queryByText("Couldn't check the storage settings"), null))

  await typeBoth(user, 'longenough1')
  await waitFor(() => assert.equal(continueButton().disabled, false))
})

test('continue stays disabled while the check for the latest edit is in flight', async () => {
  const user = userEvent.setup({ document })

  backend.storageFails = false
  renderWizard()
  await view().findByRole('button', { name: 'Continue' })
  await typeBoth(user, 'longenough1')
  await waitFor(() => assert.equal(continueButton().disabled, false))

  const gate = Promise.withResolvers<void>()
  const before = prepareCalls()

  backend.prepareGate = gate.promise
  await typeBoth(user, 'x')
  assert.equal(continueButton().disabled, true)

  await waitFor(() => assert.ok(prepareCalls() > before))
  assert.equal(continueButton().disabled, true)

  await act(async () => gate.resolve())
  await waitFor(() => assert.equal(continueButton().disabled, false))
})

test('cancelling after an edit asks before closing', async () => {
  const user = userEvent.setup({ document })
  const closed = { count: 0 }

  backend.storageFails = false
  renderWizard({
    onClose: () => {
      closed.count += 1
    },
  })
  await view().findByRole('button', { name: 'Continue' })
  await user.type(passwordField(), 'x')
  await user.click(view().getByRole('button', { name: 'Cancel' }))

  const keep = await view().findByRole('dialog', { name: 'Discard changes?' })
  assert.equal(closed.count, 0)
  await user.click(within(keep).getByRole('button', { name: 'Keep editing' }))
  await waitFor(() => assert.equal(keep.isConnected, false), { timeout: 3000 })
  assert.equal(closed.count, 0)

  await user.click(view().getByRole('button', { name: 'Cancel' }))
  const discard = await view().findByRole('dialog', { name: 'Discard changes?' })
  await user.click(within(discard).getByRole('button', { name: 'Discard changes' }))
  await waitFor(() => assert.equal(closed.count, 1))
})

test('cancelling an untouched wizard closes without asking', async () => {
  const user = userEvent.setup({ document })
  const closed = { count: 0 }

  backend.storageFails = false
  renderWizard({
    onClose: () => {
      closed.count += 1
    },
  })
  await view().findByRole('button', { name: 'Continue' })
  await user.click(view().getByRole('button', { name: 'Cancel' }))

  await waitFor(() => assert.equal(closed.count, 1))
  assert.equal(view().queryByRole('dialog', { name: 'Discard changes?' }), null)
})

test('closing the finished wizard does not ask to discard', async () => {
  const user = userEvent.setup({ document })
  const completed = { count: 0 }

  backend.storageFails = false
  renderWizard({
    onComplete: () => {
      completed.count += 1
    },
  })
  await view().findByRole('button', { name: 'Continue' })
  await typeBoth(user, 'longenough1')
  await continueWhenReady(user)
  await user.click(await view().findByRole('button', { name: 'Save' }))
  await view().findByText('Add providers to start monitoring pipelines')

  await user.click(view().getByRole('button', { name: 'Close dialog' }))

  await waitFor(() => assert.equal(completed.count, 1))
  assert.equal(view().queryByRole('dialog', { name: 'Discard changes?' }), null)
})

test('a save error disappears when the user edits the storage settings', async () => {
  const user = userEvent.setup({ document })

  backend.storageFails = false
  backend.applyFails = true
  renderWizard()
  await view().findByRole('button', { name: 'Continue' })
  await typeBoth(user, 'longenough1')
  await continueWhenReady(user)
  await user.click(await view().findByRole('button', { name: 'Save' }))
  await view().findByText("Couldn't save storage settings")

  await user.click(view().getByRole('button', { name: 'Back' }))
  await typeBoth(user, 'x')
  await continueWhenReady(user)

  await view().findByRole('button', { name: 'Save' })
  assert.equal(view().queryByText("Couldn't save storage settings"), null)
})
