import assert from 'node:assert/strict'
import { test } from 'node:test'

import { initialDraft, toFieldErrors, toSetupDraft } from './setupDraft'
import type { SetupState } from './types'

const draft = (overrides: Partial<SetupState> = {}): SetupState => ({
  backend: 'sqlite',
  dataDir: '/data',
  postgresUrl: '',
  vaultPassword: '',
  vaultPasswordConfirm: '',
  transferData: true,
  ...overrides,
})

test('the initial draft uses the default directory on first run', () => {
  assert.deepEqual(initialDraft(null, '/default'), draft({ dataDir: '/default' }))
})

test('the draft is sent to the backend with snake case fields', () => {
  assert.deepEqual(
    toSetupDraft(draft({ postgresUrl: 'postgres://h/db', vaultPassword: 'a', vaultPasswordConfirm: 'b' })),
    {
      backend: 'sqlite',
      data_dir: '/data',
      postgres_url: 'postgres://h/db',
      vault_password: 'a',
      vault_password_confirm: 'b',
      transfer_data: true,
    }
  )
})

test('backend errors map to the form fields', () => {
  assert.deepEqual(toFieldErrors({ postgres_url: 'bad url', vault_password_confirm: 'mismatch', other: 'x' }), {
    postgresUrl: 'bad url',
    vaultPasswordConfirm: 'mismatch',
  })
})
