import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formReducer } from './reducer'
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
