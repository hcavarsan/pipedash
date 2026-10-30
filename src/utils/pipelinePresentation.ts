import { STATUS_OPTIONS, type StatusKey } from '../theme/status'
import type { Pipeline, PipelineRun, PluginMetadata } from '../types/domain'
import { getPluginByType } from './pluginHelpers'

export interface PipelineLabels {
  item: string
  items: string
  group: string
  groups: string
  groupTitle: string
  sourceTitle: string
  history: string
  trigger: string
  run: string
  runs: string
}

const pipelineLabels: PipelineLabels = {
  item: 'pipeline',
  items: 'pipelines',
  group: 'organization',
  groups: 'organizations',
  groupTitle: 'Organization',
  sourceTitle: 'Repository',
  history: 'Run history',
  trigger: 'Trigger',
  run: 'run',
  runs: 'runs',
}

const fluxLabels: PipelineLabels = {
  item: 'resource',
  items: 'resources',
  group: 'namespace',
  groups: 'namespaces',
  groupTitle: 'Namespace',
  sourceTitle: 'Source',
  history: 'Reconciliation history',
  trigger: 'Reconcile',
  run: 'reconciliation',
  runs: 'reconciliations',
}

export const getPipelineLabels = (providerType?: string): PipelineLabels =>
  providerType === 'fluxcd' ? fluxLabels : pipelineLabels

export const getPipelineStatusLabel = (
  pipeline: Pick<Pipeline, 'overdue_run' | 'resource_state'>
): string | undefined => (pipeline.overdue_run ? 'Running longer than usual' : pipeline.resource_state?.label)

export const pipelineStatusKey = (pipeline: Pick<Pipeline, 'status' | 'last_run' | 'resource_state'>): StatusKey =>
  pipeline.status === 'pending' && !pipeline.last_run && !pipeline.resource_state ? 'never' : pipeline.status

export const getRunStatusLabel = (run: Pick<PipelineRun, 'metadata'>): string | undefined => {
  const label = run.metadata?.status_label

  if (typeof label !== 'string') {
    return undefined
  }

  return run.metadata?.archived === true ? `${label} (cached)` : label
}

const fluxStatusOptions = [
  { value: 'success', label: 'Ready' },
  { value: 'failed', label: 'Not ready' },
  { value: 'degraded', label: 'Degraded' },
  { value: 'running', label: 'Reconciling' },
  { value: 'pending', label: 'Pending' },
  { value: 'skipped', label: 'Suspended' },
]

export const getStatusOptions = (providerType?: string): { value: string; label: string }[] =>
  providerType === 'fluxcd' ? fluxStatusOptions : STATUS_OPTIONS

const isDynamicGitHubWorkflow = (pipeline: Pick<Pipeline, 'provider_type' | 'workflow_file'>) =>
  pipeline.provider_type === 'github' && pipeline.workflow_file?.startsWith('dynamic/') === true

export const canRerunInPlace = (pipeline: Pick<Pipeline, 'provider_type'>): boolean =>
  pipeline.provider_type === 'github'

export const READ_ONLY_REASON =
  'This provider connects without credentials, so it is read-only. Edit it and choose an access token or single sign-on to start or stop runs.'

export const isReadOnlyPipeline = (pipeline: Pick<Pipeline, 'metadata'>): boolean =>
  pipeline.metadata?.read_only === true

export const canCancelPipeline = (
  plugins: PluginMetadata[] | undefined,
  pipeline: Pick<Pipeline, 'metadata' | 'provider_type'>
): boolean => !isReadOnlyPipeline(pipeline) && !!getPluginByType(plugins, pipeline.provider_type)?.capabilities.cancel

export const getReconcileBlockReason = (pipeline: Pick<Pipeline, 'metadata'>): string | undefined => {
  if (isReadOnlyPipeline(pipeline)) {
    return READ_ONLY_REASON
  }
  if (pipeline.metadata?.missing === true) {
    return 'This resource was not found. Update the selection or recreate it in Flux.'
  }
  if (pipeline.metadata?.suspended === true) {
    return 'This resource is suspended. Resume it in Flux before requesting reconciliation.'
  }

  return undefined
}

export const getTriggerBlockReason = (
  pipeline: Pick<Pipeline, 'metadata' | 'provider_type' | 'workflow_file'>
): string | undefined =>
  isDynamicGitHubWorkflow(pipeline)
    ? 'GitHub starts this built-in workflow itself. It has no workflow file, so only its existing runs can be rerun.'
    : getReconcileBlockReason(pipeline)
