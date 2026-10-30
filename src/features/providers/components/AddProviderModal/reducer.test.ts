import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formReducer, isFormDirty } from './reducer'
import { initialFormState } from './types'

const fluxPlugin = {
  name: 'FluxCD',
  provider_type: 'fluxcd',
  version: '0.3.0',
  description: '',
  author: null,
  icon: null,
  config_schema: { fields: [] },
  table_schema: { tables: [] },
  required_permissions: [],
  features: [],
  capabilities: {
    pipelines: true,
    pipeline_runs: true,
    trigger: true,
    cancel: false,
    agents: false,
    artifacts: false,
    queues: false,
    custom_tables: false,
    retains_run_history: true,
    logs: false,
    sign_in: false,
  },
}

test('selecting resources in another namespace preserves earlier selections', () => {
  const previous = new Set(['Kustomization__team-a__web'])
  const next = formReducer(
    { ...initialFormState, selectedPipelines: previous },
    {
      type: 'SELECT_ALL_PIPELINES',
      pipelineIds: ['HelmRelease__team-b__api'],
    }
  )

  assert.deepEqual([...next.selectedPipelines], ['Kustomization__team-a__web', 'HelmRelease__team-b__api'])
  assert.deepEqual([...previous], ['Kustomization__team-a__web'])
})

test('a select all run that outlives a step change cannot finish the next run', () => {
  const steps = (state: typeof initialFormState, ...actions: Parameters<typeof formReducer>[1][]) =>
    actions.reduce(formReducer, state)
  const second = steps(
    { ...initialFormState, step: 'pipelines' },
    { type: 'START_SELECT_ALL', run: 1 },
    { type: 'SET_STEP', step: 'credentials' },
    { type: 'SET_STEP', step: 'pipelines' },
    { type: 'START_SELECT_ALL', run: 2 }
  )
  const stale = formReducer(second, { type: 'FINISH_SELECT_ALL', run: 1, pipelineIds: ['old'] })

  assert.equal(stale.selectingAll, true)
  assert.deepEqual([...stale.selectedPipelines], [])

  const done = formReducer(stale, { type: 'FINISH_SELECT_ALL', run: 2, pipelineIds: ['new'] })

  assert.equal(done.selectingAll, false)
  assert.deepEqual([...done.selectedPipelines], ['new'])
})

test('initial edit selection comes from saved resource ids before discovery finishes', () => {
  const next = formReducer(initialFormState, {
    type: 'INIT_EDIT_MODE',
    plugin: fluxPlugin,
    provider: {
      id: 1,
      name: 'Flux',
      provider_type: 'fluxcd',
      token: '',
      refresh_interval: 30,
      config: { selected_items: 'Kustomization__team-a__web, HelmRelease__team-b__api' },
    },
  })

  assert.deepEqual([...next.selectedPipelines], ['Kustomization__team-a__web', 'HelmRelease__team-b__api'])
})

test('deselecting visible resources preserves selections outside the current namespace', () => {
  const next = formReducer(
    { ...initialFormState, selectedPipelines: new Set(['team-a', 'team-b']) },
    {
      type: 'DESELECT_PIPELINES',
      pipelineIds: ['team-a'],
    }
  )

  assert.deepEqual([...next.selectedPipelines], ['team-b'])
})

test('changing the Flux kubeconfig clears its previous context', () => {
  const next = formReducer(
    {
      ...initialFormState,
      selectedPlugin: fluxPlugin,
      configValues: { kubeconfig: '/old/config', context: 'production' },
    },
    {
      type: 'UPDATE_CONFIG',
      key: 'kubeconfig',
      value: '/new/config',
    }
  )

  assert.deepEqual(next.configValues, { kubeconfig: '/new/config', context: '' })
})

for (const key of ['kubeconfig', 'context', 'namespaces']) {
  test(`changing ${key} clears saved and pending resource selections`, () => {
    const selectedId = 'Kustomization__team-a__web'
    const state = {
      ...initialFormState,
      selectedPlugin: fluxPlugin,
      configValues: { [key]: 'old-scope', selected_items: selectedId },
      selectedOrganization: 'team-a',
      selectedPipelines: new Set([selectedId]),
      repositorySearch: 'web',
    }
    const next = formReducer(state, { type: 'UPDATE_CONFIG', key, value: 'new-scope' })

    assert.equal(next.configValues.selected_items, undefined)
    assert.equal(next.selectedPipelines.size, 0)
    assert.equal(next.selectedOrganization, '')
    assert.equal(next.repositorySearch, '')
    assert.equal(state.selectedPipelines.has(selectedId), true)
  })
}

test('editing credentials without changing scope preserves resource selections', () => {
  const selectedId = 'Kustomization__team-a__web'
  const state = {
    ...initialFormState,
    selectedPlugin: fluxPlugin,
    configValues: { namespaces: 'team-a', selected_items: selectedId },
    selectedPipelines: new Set([selectedId]),
  }

  for (const action of [
    { type: 'UPDATE_CONFIG', key: 'token', value: 'updated' },
    { type: 'UPDATE_CONFIG', key: 'namespaces', value: 'team-a' },
  ] as const) {
    const next = formReducer(state, action)
    assert.equal(next.configValues.selected_items, selectedId)
    assert.deepEqual(next.selectedPipelines, state.selectedPipelines)
  }
})

test('form is dirty only after changes against the provider type defaults', () => {
  const selected = formReducer(initialFormState, { type: 'SELECT_PLUGIN', plugin: fluxPlugin, isEditMode: false })

  assert.equal(isFormDirty(initialFormState), false)
  assert.equal(isFormDirty(selected), false)
  assert.equal(isFormDirty(formReducer(selected, { type: 'SET_PROVIDER_NAME', name: 'Prod' })), true)
  assert.equal(isFormDirty(formReducer(selected, { type: 'UPDATE_CONFIG', key: 'namespaces', value: 'a' })), true)
  assert.equal(isFormDirty(formReducer(selected, { type: 'TOGGLE_PIPELINE', pipelineId: 'x' })), true)
})

test('edit form is dirty against the saved provider and ignores cleared empty values', () => {
  const provider = {
    id: 1,
    name: 'Flux',
    provider_type: 'fluxcd',
    token: '',
    refresh_interval: 30,
    config: { namespaces: 'team-a', selected_items: 'x' },
  }
  const loaded = formReducer(initialFormState, { type: 'INIT_EDIT_MODE', plugin: fluxPlugin, provider })
  const typedAndCleared = formReducer(formReducer(loaded, { type: 'UPDATE_CONFIG', key: 'token', value: 'a' }), {
    type: 'UPDATE_CONFIG',
    key: 'token',
    value: '',
  })

  assert.equal(isFormDirty(loaded, provider), false)
  assert.equal(isFormDirty(typedAndCleared, provider), false)
  assert.equal(isFormDirty(formReducer(loaded, { type: 'SET_PROVIDER_NAME', name: 'Other' }), provider), true)
})

test('edit form shows the display name of a provider defined in the config file', () => {
  const provider = {
    id: 1,
    name: 'ci-a',
    provider_type: 'fluxcd',
    token: '',
    refresh_interval: 30,
    config: { display_name: 'Renamed CI' },
  }

  const loaded = formReducer(initialFormState, { type: 'INIT_EDIT_MODE', plugin: fluxPlugin, provider })

  assert.equal(loaded.providerName, 'Renamed CI')
  assert.equal(isFormDirty(loaded, provider), false)
})

test('switching to sign-in drops the token and keeps selected pipelines', () => {
  const next = formReducer(
    {
      ...initialFormState,
      configValues: { token: 'abc', server_url: 'https://argo' },
      selectedPipelines: new Set(['a']),
    },
    { type: 'SET_AUTH_MODE', mode: 'sign_in' }
  )

  assert.equal(next.authMode, 'sign_in')
  assert.deepEqual(next.configValues, { server_url: 'https://argo' })
  assert.deepEqual([...next.selectedPipelines], ['a'])
})

test('editing a provider with a stored sign-in starts in sign-in mode', () => {
  const next = formReducer(initialFormState, {
    type: 'INIT_EDIT_MODE',
    plugin: fluxPlugin,
    provider: {
      id: 3,
      name: 'Argo',
      provider_type: 'argocd',
      token: '',
      refresh_interval: 30,
      config: { sign_in: 'provider:3' },
    },
  })

  assert.equal(next.authMode, 'sign_in')
  assert.equal(next.configValues.sign_in, 'provider:3')
})

test('config updates leave the previous config, errors and selection untouched', () => {
  const state = {
    ...initialFormState,
    selectedPlugin: fluxPlugin,
    configValues: { namespaces: 'a', selected_items: 'x' },
    fieldErrors: { namespaces: 'bad', token: 'missing' },
    selectedPipelines: new Set(['x']),
  }
  const next = formReducer(state, { type: 'UPDATE_CONFIG', key: 'namespaces', value: 'b' })

  assert.deepEqual(state.configValues, { namespaces: 'a', selected_items: 'x' })
  assert.deepEqual(state.fieldErrors, { namespaces: 'bad', token: 'missing' })
  assert.deepEqual([...state.selectedPipelines], ['x'])
  assert.deepEqual(next.fieldErrors, { token: 'missing' })

  const unchanged = formReducer(state, { type: 'UPDATE_CONFIG', key: 'namespaces', value: 'a' })

  assert.equal(unchanged.configValues.selected_items, 'x')
  assert.deepEqual([...unchanged.selectedPipelines], ['x'])
})

test('clearing a field error keeps the others and the previous object', () => {
  const state = { ...initialFormState, fieldErrors: { a: '1', b: '2' } }
  const next = formReducer(state, { type: 'CLEAR_FIELD_ERROR', key: 'a' })

  assert.deepEqual(next.fieldErrors, { b: '2' })
  assert.deepEqual(state.fieldErrors, { a: '1', b: '2' })
})

test('selecting a provider type applies schema defaults and clears stale selections', () => {
  const plugin = {
    ...fluxPlugin,
    config_schema: {
      fields: [
        {
          key: 'namespaces',
          label: 'Namespaces',
          description: null,
          field_type: 'Text' as const,
          required: false,
          default_value: 'flux-system',
          options: null,
          validation_regex: null,
          validation_message: null,
          section: 'settings' as const,
        },
      ],
    },
  }
  const next = formReducer(
    { ...initialFormState, selectedPipelines: new Set(['x']), fieldErrors: { a: 'b' } },
    { type: 'SELECT_PLUGIN', plugin, isEditMode: false }
  )

  assert.deepEqual(next.configValues, { namespaces: 'flux-system' })
  assert.equal(next.providerName, 'FluxCD')
  assert.equal(next.selectedPipelines.size, 0)
  assert.deepEqual(next.fieldErrors, {})
})

test('clearing the provider type and resetting return to an empty form', () => {
  const filled = {
    ...initialFormState,
    selectedPlugin: fluxPlugin,
    providerName: 'Prod',
    configValues: { token: 'a' },
    selectedPipelines: new Set(['x']),
    step: 'pipelines' as const,
  }
  const cleared = formReducer(filled, { type: 'CLEAR_PLUGIN' })

  assert.equal(cleared.selectedPlugin, null)
  assert.equal(cleared.providerName, '')
  assert.deepEqual(cleared.configValues, {})
  assert.equal(cleared.selectedPipelines.size, 0)
  assert.deepEqual(formReducer(filled, { type: 'RESET' }), initialFormState)
})

const noAuthPlugin = {
  ...fluxPlugin,
  provider_type: 'prow',
  name: 'Prow',
  config_schema: { fields: [], no_auth: true },
}

test('a provider type that offers no auth starts in none mode, others start in token mode', () => {
  const none = formReducer(initialFormState, { type: 'SELECT_PLUGIN', plugin: noAuthPlugin, isEditMode: false })
  const token = formReducer(initialFormState, { type: 'SELECT_PLUGIN', plugin: fluxPlugin, isEditMode: false })

  assert.equal(none.authMode, 'none')
  assert.equal(none.configValues.auth_mode, 'none')
  assert.equal(token.authMode, 'token')
  assert.equal('auth_mode' in token.configValues, false)
})

test('switching auth mode drops the other credentials and tracks auth_mode', () => {
  const base = {
    ...initialFormState,
    configValues: { token: 'abc', sign_in: 'provider:1', deck_url: 'https://deck' },
    fieldErrors: { token: 'bad', sign_in: 'bad', deck_url: 'bad' },
  }
  const none = formReducer(base, { type: 'SET_AUTH_MODE', mode: 'none' })

  assert.equal(none.authMode, 'none')
  assert.deepEqual(none.configValues, { deck_url: 'https://deck', auth_mode: 'none' })
  assert.deepEqual(none.fieldErrors, { deck_url: 'bad' })

  const token = formReducer(
    { ...none, configValues: { ...none.configValues, sign_in: 'provider:1' } },
    { type: 'SET_AUTH_MODE', mode: 'token' }
  )

  assert.deepEqual(token.configValues, { deck_url: 'https://deck' })

  const signIn = formReducer(
    { ...none, configValues: { ...none.configValues, token: 'abc' } },
    { type: 'SET_AUTH_MODE', mode: 'sign_in' }
  )

  assert.deepEqual(signIn.configValues, { deck_url: 'https://deck' })
})

test('editing a provider saved without credentials restores none mode', () => {
  const next = formReducer(initialFormState, {
    type: 'INIT_EDIT_MODE',
    plugin: noAuthPlugin,
    provider: {
      id: 4,
      name: 'Prow',
      provider_type: 'prow',
      token: '',
      refresh_interval: 30,
      config: { auth_mode: 'none' },
    },
  })

  assert.equal(next.authMode, 'none')
  assert.equal(next.configValues.auth_mode, 'none')
})
