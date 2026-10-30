import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { PluginMetadata } from '../types/domain'
import {
  canCancelPipeline,
  getPipelineStatusLabel,
  getReconcileBlockReason,
  getRunStatusLabel,
  getTriggerBlockReason,
  READ_ONLY_REASON,
} from './pipelinePresentation'

test('pipeline label prefers overdue over the resource state label', () => {
  assert.equal(
    getPipelineStatusLabel({ overdue_run: 'run-1', resource_state: { label: 'Synced' } }),
    'Running longer than usual'
  )
  assert.equal(getPipelineStatusLabel({ resource_state: { label: 'Synced' } }), 'Synced')
  assert.equal(getPipelineStatusLabel({}), undefined)
})

test('run label comes from run metadata and marks archived runs as cached', () => {
  assert.equal(getRunStatusLabel({ metadata: { status_label: 'Reconciling' } }), 'Reconciling')
  assert.equal(getRunStatusLabel({ metadata: { status_label: 'Ready', archived: true } }), 'Ready (cached)')
  assert.equal(getRunStatusLabel({}), undefined)
})

test('read-only pipelines block triggering with the reason and refuse cancelling', () => {
  const plugin = (cancel: boolean): PluginMetadata[] => [
    {
      name: 'Prow',
      provider_type: 'prow',
      version: '0.1.0',
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
        trigger: false,
        cancel,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
        logs: false,
        sign_in: false,
      },
    },
  ]
  const plugins = plugin(true)
  const open = { provider_type: 'prow', workflow_file: null, metadata: {} }
  const readOnly = { ...open, metadata: { read_only: true } }

  assert.equal(getTriggerBlockReason(readOnly), READ_ONLY_REASON)
  assert.equal(getReconcileBlockReason(readOnly), READ_ONLY_REASON)
  assert.equal(getTriggerBlockReason(open), undefined)
  assert.equal(canCancelPipeline(plugins, readOnly), false)
  assert.equal(canCancelPipeline(plugins, open), true)
  assert.equal(canCancelPipeline(plugin(false), open), false)
})
