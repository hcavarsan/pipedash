import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { AuthMode, ConfigField } from '../../../../types/domain'

import {
  activeConfigFields,
  activeConfigValues,
  mapProviderFieldErrors,
  validateConfigField,
  validateProviderCredentials,
  validateProviderName,
  validateSignIn,
} from './validation'

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

const configField = (key: string): ConfigField => ({
  key,
  label: key,
  description: null,
  field_type: 'Text',
  required: false,
  default_value: null,
  options: null,
  validation_regex: null,
  validation_message: null,
})

test('sign-in mode drops the token field and needs a completed sign-in', () => {
  const fields = [configField('server_url'), configField('token')]

  assert.deepEqual(
    activeConfigFields(fields, 'sign_in', undefined).map((field) => field.key),
    ['server_url']
  )
  assert.equal(activeConfigFields(fields, 'token', undefined).length, 2)
  assert.equal(validateSignIn('sign_in', {}, undefined), 'Sign in to continue')
  assert.equal(validateSignIn('sign_in', { sign_in: 'abc' }, { state: 'pending' }), 'Finish signing in to continue')
  assert.equal(validateSignIn('sign_in', { sign_in: 'abc' }, { state: 'completed', account: null }), null)
  assert.equal(validateSignIn('token', {}, undefined), null)
})

test('config fields: whitespace fails when required and empty passes when optional', () => {
  const required = { ...configField('token'), required: true }

  assert.equal(validateConfigField(required, '   '), 'token is required')
  assert.equal(validateConfigField(configField('note'), ''), null)
})

test('config fields: regex mismatch uses the custom message, then the fallback', () => {
  const field = { ...configField('url'), validation_regex: '^https://' }

  assert.equal(validateConfigField(field, 'https://a.example'), null)
  assert.equal(validateConfigField(field, 'http://a.example'), 'Invalid format for url')
  assert.equal(validateConfigField({ ...field, validation_message: 'Use https' }, 'x'), 'Use https')
})

test('provider name must have non-whitespace content', () => {
  assert.equal(validateProviderName('  '), 'Provider name is required')
  assert.equal(validateProviderName('GitHub'), null)
})

test('credentials draft collects name, field and sign-in errors together', () => {
  const fields = [{ ...configField('token'), required: true }]

  assert.deepEqual(
    validateProviderCredentials({
      providerName: '',
      fields,
      authMode: 'token',
      authModes: undefined,
      configValues: {},
      signInStatus: undefined,
    }),
    { providerName: 'Provider name is required', token: 'token is required' }
  )
  assert.deepEqual(
    validateProviderCredentials({
      providerName: 'A',
      fields,
      authMode: 'sign_in',
      authModes: undefined,
      configValues: {},
      signInStatus: undefined,
    }),
    { sign_in: 'Sign in to continue' }
  )
})

test('auth-scoped fields count only in their own mode', () => {
  const fields = [
    configField('deck_url'),
    { ...configField('sso_issuer'), required: true },
    configField('auth_scheme'),
    configField('token'),
  ]
  const authModes: Record<string, AuthMode[]> = { sso_issuer: ['sign_in'], auth_scheme: ['token'] }

  assert.deepEqual(
    activeConfigFields(fields, 'token', authModes).map((field) => field.key),
    ['deck_url', 'auth_scheme', 'token']
  )
  assert.deepEqual(
    activeConfigFields(fields, 'sign_in', authModes).map((field) => field.key),
    ['deck_url', 'sso_issuer']
  )
  assert.deepEqual(activeConfigValues({ deck_url: 'd', sso_issuer: 'i', auth_scheme: 'basic' }, 'sign_in', authModes), {
    deck_url: 'd',
    sso_issuer: 'i',
  })
  assert.deepEqual(
    validateProviderCredentials({
      providerName: 'P',
      fields,
      authMode: 'token',
      authModes,
      configValues: {},
      signInStatus: undefined,
    }),
    {}
  )
})

test('none mode hides the token and mode-scoped fields, keeps unscoped fields and auth_mode', () => {
  const fields = [configField('deck_url'), configField('token'), configField('sso_issuer'), configField('auth_scheme')]
  const authModes: Record<string, AuthMode[]> = { sso_issuer: ['sign_in'], auth_scheme: ['token', 'sign_in'] }

  assert.deepEqual(
    activeConfigFields(fields, 'none', authModes).map((field) => field.key),
    ['deck_url']
  )
  assert.deepEqual(
    activeConfigFields(fields, 'sign_in', authModes).map((field) => field.key),
    ['deck_url', 'sso_issuer', 'auth_scheme']
  )
  assert.deepEqual(
    activeConfigValues({ deck_url: 'd', sso_issuer: 'i', auth_scheme: 'basic', auth_mode: 'none' }, 'none', authModes),
    { deck_url: 'd', auth_mode: 'none' }
  )
  assert.deepEqual(
    validateProviderCredentials({
      providerName: 'P',
      fields: fields.map((field) => ({ ...field, required: true })),
      authMode: 'none',
      authModes,
      configValues: { deck_url: 'd' },
      signInStatus: undefined,
    }),
    {}
  )
})
