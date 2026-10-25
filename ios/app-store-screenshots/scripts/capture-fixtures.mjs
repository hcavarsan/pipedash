const now = new Date('2026-10-25T09:41:00Z').getTime()
const ago = (minutes) => new Date(now - minutes * 60_000).toISOString()
const healthy = { level: 'operational', signals: [], next_attempt_at: null, in_flight: false, status_page: null }
const components = [
  { name: 'Actions', level: 'partial_outage', detail: 'Degraded performance' },
  { name: 'API Requests', level: 'operational', detail: 'Operational' },
  { name: 'Git Operations', level: 'operational', detail: 'Operational' },
]
const incident = {
  id: 'demo-actions-delay', name: 'Actions jobs are delayed', level: 'partial_outage', phase: 'Identified',
  latest_update: 'The cause has been identified. A fix is being deployed to restore normal job processing.',
  url: 'https://www.githubstatus.com/', started_at: ago(27), updated_at: ago(3),
}
export const providers = [
  ['GitHub', 'github'], ['GitLab', 'gitlab'], ['Jenkins', 'jenkins'], ['Argo CD', 'argocd'],
].map(([name, provider_type], index) => ({
  id: index + 1, name, provider_type, icon: `/provider-icons/${provider_type}.svg`, pipeline_count: 2,
  last_updated: ago(1), refresh_interval: 60, configured_repositories: [], last_fetch_status: 'success',
  last_fetch_error: null, last_fetch_error_kind: null, last_fetch_at: ago(1), paused_until: null,
  health: index === 0 ? {
    ...healthy, level: 'partial_outage',
    status_page: { name: 'GitHub', url: 'https://www.githubstatus.com/', checked_at: ago(1), error: null, components, other_incidents: [] },
    signals: [{ kind: 'status_page', episode_id: 'demo-actions-delay', level: 'partial_outage', evidence: 'fresh',
      observed_at: ago(1), title: 'Actions jobs are delayed', detail: 'Jobs may take longer to start.',
      reported_by: 'GitHub Status', url: 'https://www.githubstatus.com/', components, incidents: [incident],
      maintenances: [], updated_at: ago(3) }],
  } : { ...healthy },
}))
export const pipelines = [
  [1, 'Release iOS app', 'success', 'acme/mobile', 'main', 4],
  [1, 'API integration tests', 'failed', 'acme/platform-api', 'main', 8],
  [2, 'Deploy storefront', 'running', 'acme/storefront', 'release/2.8', 2],
  [2, 'Security scan', 'success', 'acme/platform-api', 'main', 12],
  [3, 'Build Android app', 'success', 'acme/mobile', 'main', 17],
  [3, 'Publish SDK', 'pending', 'acme/developer-sdk', 'main', 1],
  [4, 'Production cluster', 'success', 'acme/infrastructure', 'main', 6],
  [4, 'Staging cluster', 'success', 'acme/infrastructure', 'develop', 9],
].map(([provider_id, name, status, repository, branch, minutes], index) => ({
  id: `${providers[provider_id - 1].provider_type}__${provider_id}__${index + 1}`,
  provider_id, provider_type: providers[provider_id - 1].provider_type, name, status, repository, branch,
  last_run: ago(minutes), last_updated: ago(1), workflow_file: '.github/workflows/release.yml',
  metadata: { organization: 'acme', html_url: 'https://example.com/demo' },
}))
export const release = pipelines[0]
export const failure = pipelines[1]
export const parameters = [
  { name: 'environment', label: 'Environment', description: 'Choose the deployment target', type: 'choice', default: 'Production', options: ['Staging', 'Production'], required: true },
  { name: 'version', label: 'Release version', description: 'The version to publish', type: 'string', default: '2.8.0', required: true },
  { name: 'ref', label: 'Branch', description: 'Build from this branch', type: 'string', default: 'main', required: true },
  { name: 'run_tests', label: 'Run tests before release', description: null, type: 'boolean', default: true, required: false },
]
export function runsFor(pipeline) {
  return Array.from({ length: 8 }, (_, index) => ({
    id: `demo-${pipeline.id}-${248 - index}`, pipeline_id: pipeline.id, run_number: 248 - index,
    status: index === 0 ? pipeline.status : index === 4 ? 'failed' : 'success',
    started_at: ago(index * 53 + 8), concluded_at: ago(index * 53 + 4), duration_seconds: 242 + index * 7,
    logs_url: 'https://example.com/demo/run', commit_sha: ['a8f3c21', '7b2d4e9', 'f9c6a12'][index % 3],
    commit_message: index === 0 && pipeline.id === failure.id ? 'Validate checkout API contract' : ['Ship release 2.8.0', 'Improve deployment checks', 'Update app configuration'][index % 3],
    branch: 'main', actor: ['alex', 'sam', 'jordan'][index % 3],
    inputs: { environment: 'Production', version: '2.8.0', ref: 'main', run_tests: true }, metadata: {},
  }))
}
const column = (id, label, data_type, renderer, width = null) => ({
  id, label, description: null, field_path: id, data_type, renderer, visibility: 'Always', default_visible: true, width, sortable: true, filterable: false,
})
export const tableSchema = { tables: [{
  id: 'pipeline_runs', name: 'Run history', description: null, default_sort_column: 'run_number', default_sort_direction: 'desc',
  columns: [column('run_number', 'Run', 'Number', 'Text', 75), column('status', 'Status', 'Status', 'StatusBadge', 110),
    column('commit_message', 'Commit', 'String', 'TruncatedText', 220), column('branch', 'Branch', 'String', 'Badge', 90),
    column('actor', 'Actor', 'String', 'Text', 80), column('duration_seconds', 'Duration', 'Duration', 'Duration', 90),
    column('started_at', 'Started', 'DateTime', 'DateTime', 130)],
}] }
export const plugins = providers.map(({ name, provider_type, icon }) => ({
  name, provider_type, icon, version: '0.4.0', description: `${name} workflows`, author: 'Pipedash',
  config_schema: { fields: [] }, table_schema: tableSchema, required_permissions: [], features: [],
  capabilities: { pipelines: true, pipeline_runs: true, trigger: true, cancel: true, agents: false, artifacts: false,
    queues: false, custom_tables: false, retains_run_history: true },
}))
export const notifications = [
  { key: 'demo-failed-run', at: now - 8 * 60_000, read: false, tone: 'danger', level: null,
    title: 'API integration tests failed', body: 'Run #248 failed on main. Review the run before your next release.',
    providerIds: [1], pipelineId: failure.id, link: null },
  { key: 'demo-provider-incident', at: now - 27 * 60_000, read: false, tone: 'warning', level: 'partial_outage',
    title: 'GitHub Actions jobs are delayed', body: 'GitHub reports degraded Actions performance. Your workflows may take longer to start.',
    providerIds: [1], pipelineId: null, link: { url: 'https://www.githubstatus.com/', kind: 'incident' } },
  { key: 'demo-recovered', at: now - 74 * 60_000, read: true, tone: 'success', level: 'operational',
    title: 'GitLab service recovered', body: 'Pipeline processing is back to normal.', providerIds: [2], pipelineId: null, link: null },
  { key: 'demo-failed-build', at: now - 130 * 60_000, read: true, tone: 'danger', level: null,
    title: 'Build Android app failed', body: 'Run #193 failed on main.', providerIds: [3], pipelineId: pipelines[4].id, link: null },
]
export const fixedNow = now
export function mockResponse(method, url) {
  const path = decodeURIComponent(url.pathname.replace('/api/v1', ''))
  if (method !== 'GET') throw new Error(`Unexpected mutation: ${method} ${path}`)
  if (path === '/setup/status') return { config_exists: true, config_valid: true, validation_errors: [], needs_setup: false, needs_migration: false }
  if (path === '/vault/status') return { is_unlocked: true, password_source: 'none', backend: 'sqlite', requires_password: false }
  if (path === '/providers') return providers
  if (path === '/plugins') return plugins
  if (path === '/health') return { version: '0.4.0' }
  if (path === '/pipelines/cached') return pipelines.filter((pipeline) => !url.searchParams.has('provider_id') || pipeline.provider_id === Number(url.searchParams.get('provider_id')))
  if (/^\/providers\/[1-4]\/table-schema$/.test(path)) return tableSchema
  if (/^\/providers\/[1-4]\/features$/.test(path)) return []
  if (/^\/providers\/[1-4]\/permissions$/.test(path)) return { permissions: [], all_granted: true, checked_at: ago(1) }
  if (/^\/preferences\/table\/[1-4]\/pipeline_runs$/.test(path)) return null
  if (/^\/preferences\/table\/[1-4]\/pipeline_runs\/default$/.test(path)) return { columnOrder: tableSchema.tables[0].columns.map(({ id }) => id), columnVisibility: {} }
  for (const pipeline of pipelines) {
    if (path === `/pipelines/${pipeline.id}/workflow-params`) return parameters
    const runs = runsFor(pipeline)
    if (path === `/pipelines/${pipeline.id}/runs`) return { runs, total_count: runs.length, has_more: false, is_complete: true, page: 1, page_size: 20, total_pages: 1 }
    const run = runs.find((item) => path === `/pipelines/${pipeline.id}/runs/${item.run_number}`)
    if (run) return run
  }
  throw new Error(`Unexpected API request: ${method} ${path}`)
}
