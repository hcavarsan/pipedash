import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
mockWindows('main')

interface UnlockResponse {
  success: boolean
  message?: string
  reason?: 'cancelled'
}

const status = {
  firstTime: false,
  requiresPassword: true,
  locked: true,
  biometric: false,
  fails: false,
  gate: Promise.resolve(),
}
const unlock: { calls: string[]; response: UnlockResponse; gate: Promise<void> } = {
  calls: [],
  response: { success: true },
  gate: Promise.resolve(),
}
const biometric: { calls: number; response: UnlockResponse } = { calls: 0, response: { success: true } }

mockIPC(
  (command) => {
    switch (command) {
      case 'unlock_with_biometrics':
        biometric.calls += 1

        return biometric.response
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
const { UnlockForm } = await import('./UnlockForm')
const { service } = await import('../../services')

const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

const mocks: { mock: { restore: () => void } }[] = []

beforeEach(() => {
  notifyManager.setNotifyFunction((callback) => act(callback))
  mocks.push(
    mock.method(service, 'getVaultStatus', async () => {
      await status.gate
      if (status.fails) {
        throw new Error('status unavailable')
      }

      return {
        is_unlocked: false,
        password_source: 'none',
        backend: 'sqlite',
        is_first_time: status.firstTime,
        requires_password: status.requiresPassword,
        app_lock: {
          supported: true,
          biometry: 'faceId',
          biometry_available: true,
          enabled: status.biometric,
          lock_on_background: false,
          locked: status.locked,
        },
      }
    })
  )

  mocks.push(
    mock.method(service, 'unlockVault', async (password: string) => {
      unlock.calls.push(password)
      await unlock.gate

      return { message: '', ...unlock.response }
    })
  )
  Object.assign(status, {
    firstTime: false,
    requiresPassword: true,
    locked: true,
    biometric: false,
    fails: false,
    gate: Promise.resolve(),
  })
  unlock.calls = []
  unlock.response = { success: true }
  unlock.gate = Promise.resolve()
  biometric.calls = 0
  biometric.response = { success: true }
})

afterEach(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  for (const installed of mocks.splice(0)) {
    installed.mock.restore()
  }
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const ui = (props: Parameters<typeof UnlockForm>[0]) => (
  <MantineProvider>
    <QueryClientProvider client={client}>
      <UnlockForm {...props} />
    </QueryClientProvider>
  </MantineProvider>
)

const passwordField = () => view().findByLabelText(/^Password/)
const confirmField = () => view().findByLabelText(/^Confirm password/)

function inputValue(element: HTMLElement): string {
  return element instanceof HTMLInputElement ? element.value : ''
}

function isDisabled(element: HTMLElement): boolean {
  return element instanceof HTMLButtonElement && element.disabled
}

test('the form shows a loading state until the vault status arrives', async () => {
  const { promise, resolve } = Promise.withResolvers<void>()
  status.gate = promise
  render(ui({ onSuccess: () => undefined }))

  assert.ok(await view().findByText(/Loading vault status/))
  assert.equal(view().queryByLabelText(/^Password/), null)

  await act(async () => resolve())
  assert.ok(await passwordField())
})

test('a failed status query shows an error with a retry that recovers', async () => {
  status.fails = true
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))

  assert.ok(await view().findByText("Couldn't load vault status"))

  status.fails = false
  await user.click(view().getByRole('button', { name: /retry|try again/i }))

  assert.ok(await passwordField())
})

test('nothing renders when the vault needs no unlock', async () => {
  status.requiresPassword = false
  status.locked = false
  render(ui({ onSuccess: () => undefined }))

  await waitFor(() => assert.equal(view().queryByText(/Loading vault status/), null))
  assert.equal(view().queryAllByRole('button').length, 0)
  assert.equal(view().queryByLabelText(/^Password/), null)
})

test('an empty password is rejected without calling unlock', async () => {
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))
  await passwordField()
  await user.click(view().getByRole('button', { name: 'Unlock' }))

  assert.ok(await view().findByText('Password is required'))
  assert.equal(unlock.calls.length, 0)
})

test('leaving an empty password field shows the required error before submitting', async () => {
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))
  await user.click(await passwordField())
  await user.tab()

  assert.ok(await view().findByText('Password is required'))
  assert.equal(unlock.calls.length, 0)
})

test('leaving a mismatched confirmation field shows the mismatch error before submitting', async () => {
  status.firstTime = true
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))
  await user.type(await passwordField(), 'secret')
  await user.type(await confirmField(), 'other')
  await user.tab()

  assert.ok(await view().findByText('Passwords do not match'))
  assert.equal(unlock.calls.length, 0)
})

test('a first-time mismatch is rejected on submit', async () => {
  status.firstTime = true
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))
  await user.type(await passwordField(), 'secret')
  await user.type(await confirmField(), 'other')
  await user.click(view().getByRole('button', { name: 'Unlock' }))

  assert.ok(await view().findByText('Passwords do not match'))
  assert.equal(unlock.calls.length, 0)
})

test('Enter in the first-time password field moves focus to the confirmation', async () => {
  status.firstTime = true
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))
  await user.type(await passwordField(), 'secret{Enter}')

  assert.equal(document.activeElement, await confirmField())
  assert.equal(unlock.calls.length, 0)
})

test('Enter while composing text does not move focus', async () => {
  status.firstTime = true
  render(ui({ onSuccess: () => undefined }))
  const password = await passwordField()
  password.focus()
  await act(async () => {
    password.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true, cancelable: true })
    )
  })

  assert.equal(document.activeElement, password)
})

test('matching first-time passwords unlock, clear the fields and call onSuccess once', async () => {
  status.firstTime = true
  const user = userEvent.setup({ document })
  let successes = 0
  render(ui({ onSuccess: () => (successes += 1) }))
  const password = await passwordField()
  const confirmation = await confirmField()
  await user.type(password, 'secret')
  await user.type(confirmation, 'secret')
  await user.click(view().getByRole('button', { name: 'Unlock' }))

  await waitFor(() => assert.equal(successes, 1))
  assert.deepEqual(unlock.calls, ['secret'])
  await waitFor(() => {
    assert.equal(inputValue(password), '')
    assert.equal(inputValue(confirmation), '')
  })
})

test('an unsuccessful response keeps the error visible until the user edits', async () => {
  unlock.response = { success: false, message: 'Incorrect vault password' }
  const user = userEvent.setup({ document })
  let successes = 0
  render(ui({ onSuccess: () => (successes += 1) }))
  await user.type(await passwordField(), 'wrong')
  await user.click(view().getByRole('button', { name: 'Unlock' }))

  assert.ok(await view().findByText('Incorrect vault password'))
  assert.equal(successes, 0)
  assert.equal(inputValue(await passwordField()), 'wrong')

  await user.type(await passwordField(), 'x')
  await waitFor(() => assert.equal(view().queryByText('Incorrect vault password'), null))
})

test('a pending unlock disables duplicate submission', async () => {
  const { promise, resolve } = Promise.withResolvers<void>()
  unlock.gate = promise
  const user = userEvent.setup({ document })
  render(ui({ onSuccess: () => undefined }))
  await user.type(await passwordField(), 'secret')
  await user.click(view().getByRole('button', { name: 'Unlock' }))
  await waitFor(() => assert.equal(unlock.calls.length, 1))

  const button = view().getByRole('button', { name: 'Unlock' })
  assert.equal(isDisabled(button), true)
  await user.click(button)
  assert.equal(unlock.calls.length, 1)

  await act(async () => resolve())
})

test('the cancel button calls onCancel when requested', async () => {
  const user = userEvent.setup({ document })
  let cancels = 0
  render(ui({ onSuccess: () => undefined, showCancelButton: true, onCancel: () => (cancels += 1) }))
  await passwordField()
  await user.click(view().getByRole('button', { name: 'Cancel' }))

  assert.equal(cancels, 1)
})

test('without showCancelButton there is no cancel button', async () => {
  render(ui({ onSuccess: () => undefined }))
  await passwordField()

  assert.equal(view().queryByRole('button', { name: 'Cancel' }), null)
})

test('passwordless unlock calls unlock with an empty password and onSuccess', async () => {
  status.requiresPassword = false
  const user = userEvent.setup({ document })
  let successes = 0
  render(ui({ onSuccess: () => (successes += 1) }))
  await user.click(await view().findByRole('button', { name: 'Unlock' }))

  await waitFor(() => assert.equal(successes, 1))
  assert.deepEqual(unlock.calls, [''])
  assert.equal(view().queryByLabelText(/^Password/), null)
})

test('biometric unlock prompts once on mount and calls onSuccess on success', async () => {
  status.requiresPassword = false
  status.biometric = true
  let successes = 0
  const onSuccess = () => (successes += 1)
  const { rerender } = render(ui({ onSuccess }))

  await waitFor(() => assert.equal(successes, 1))
  rerender(ui({ onSuccess: () => (successes += 1) }))
  await act(async () => undefined)

  assert.equal(biometric.calls, 1)
  assert.equal(successes, 1)
})

test('a cancelled biometric prompt shows no error and can be retried', async () => {
  status.requiresPassword = false
  status.biometric = true
  biometric.response = { success: false, message: 'User cancelled', reason: 'cancelled' }
  const user = userEvent.setup({ document })
  let successes = 0
  render(ui({ onSuccess: () => (successes += 1) }))
  const button = await view().findByRole('button', { name: /Unlock with Face ID/ })
  await waitFor(() => assert.equal(biometric.calls, 1))

  assert.equal(view().queryByText('User cancelled'), null)
  assert.equal(successes, 0)

  biometric.response = { success: true }
  await user.click(button)
  await waitFor(() => assert.equal(successes, 1))
  assert.equal(biometric.calls, 2)
})

test('a failed biometric prompt shows its message', async () => {
  status.requiresPassword = false
  status.biometric = true
  biometric.response = { success: false, message: 'Face ID did not match' }
  let successes = 0
  render(ui({ onSuccess: () => (successes += 1) }))

  assert.ok(await view().findByText('Face ID did not match'))
  assert.equal(successes, 0)
})
