import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { ConfigField } from '../../../types'

import { activeConfigFields, mapProviderFieldErrors, validateSignIn } from './validation'

test('provider validation maps names and config fields to visible credential fields', () => {
  const errors = {
    name: 'Choose a provider name',
    token: 'Check your token',
    'config.context': 'Choose a context',
    refresh_interval: 'Choose an interval',
    'config.unknown': 'Unknown setting',
  }

  assert.deepEqual(mapProviderFieldErrors(errors, [{ key: 'token' }, { key: 'context' }]), {
    providerName: 'Choose a provider name',
    token: 'Check your token',
    context: 'Choose a context',
  })
  assert.equal(errors['config.context'], 'Choose a context')
})

test('provider validation leaves unknown fields for the operation error', () => {
  assert.deepEqual(mapProviderFieldErrors({ selected_items: 'Select a pipeline' }, []), {})
})

test('sign-in mode drops the token field and needs a completed sign-in', () => {
  const fields = [{ key: 'server_url' }, { key: 'token' }] as ConfigField[]

  assert.deepEqual(
    activeConfigFields(fields, 'sign_in').map((field) => field.key),
    ['server_url']
  )
  assert.equal(activeConfigFields(fields, 'token').length, 2)
  assert.equal(validateSignIn('sign_in', {}, undefined), 'Sign in to continue')
  assert.equal(validateSignIn('sign_in', { sign_in: 'abc' }, { state: 'pending' }), 'Finish signing in to continue')
  assert.equal(validateSignIn('sign_in', { sign_in: 'abc' }, { state: 'completed', account: null }), null)
  assert.equal(validateSignIn('token', {}, undefined), null)
})
