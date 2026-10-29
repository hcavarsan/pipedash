import assert from 'node:assert/strict'
import { test } from 'node:test'

import { initialDraft, isPostgresUrl, validateSetupDraft } from './setupDraft'
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

test('only postgres URLs with a host are accepted', () => {
  assert.equal(isPostgresUrl('postgresql://u:p@localhost:5432/db'), true)
  assert.equal(isPostgresUrl(' postgres://host/db '), true)
  assert.equal(isPostgresUrl('mysql://host/db'), false)
  assert.equal(isPostgresUrl('postgres://'), false)
  assert.equal(isPostgresUrl('not a url'), false)
})

test('a postgres draft without a valid URL is rejected even when no vault password is needed', () => {
  assert.deepEqual(Object.keys(validateSetupDraft(draft({ backend: 'postgres', postgresUrl: 'nope' }), false)), [
    'postgresUrl',
  ])
})

test('the postgres URL is ignored for sqlite drafts', () => {
  assert.deepEqual(validateSetupDraft(draft({ postgresUrl: 'nope' }), false), {})
})

test('a required vault password must be at least 8 characters and confirmed', () => {
  assert.deepEqual(
    Object.keys(validateSetupDraft(draft({ vaultPassword: 'short', vaultPasswordConfirm: 'short' }), true)),
    ['vaultPassword']
  )
  assert.deepEqual(
    Object.keys(validateSetupDraft(draft({ vaultPassword: 'long enough', vaultPasswordConfirm: 'different' }), true)),
    ['vaultPasswordConfirm']
  )
  assert.deepEqual(
    validateSetupDraft(draft({ vaultPassword: 'long enough', vaultPasswordConfirm: 'long enough' }), true),
    {}
  )
})

test('the vault password is not validated when it is not needed', () => {
  assert.deepEqual(validateSetupDraft(draft({ vaultPassword: 'x', vaultPasswordConfirm: 'y' }), false), {})
})

test('the initial draft uses the default directory on first run', () => {
  assert.deepEqual(initialDraft(null, '/default'), draft({ dataDir: '/default' }))
})
