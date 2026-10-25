import assert from 'node:assert/strict'
import { test } from 'node:test'

import { mapProviderFieldErrors } from './validation'

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
