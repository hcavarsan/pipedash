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
  last_fetch_error: null, last_fetch_error_kind: null, last_fetch_at: ago(1), last_success_at: ago(index === 0 ? 95 : 1), paused_until: null,
  freshness: index === 0 ? { state: 'down', stale_at: null } : { state: 'fresh', stale_at: ago(-9) },
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
    queues: false, custom_tables: false, retains_run_history: true, logs: provider_type === 'github' },
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
const job = (id, name, status, duration, log = 'ready') => ({
  id, parent_id: null, kind: 'job', name, status, started_at: ago(8), finished_at: ago(4), duration_seconds: duration,
  log, web_url: 'https://example.com/demo/job', depth: 0, navigable: true,
})
export const logJob = job('contract-tests', 'Contract tests', 'failed', 151)
const logTree = {
  nodes: [job('build', 'Build', 'success', 48), job('unit-tests', 'Unit tests', 'success', 63), logJob, job('lint', 'Lint', 'success', 22)],
  default_node_id: logJob.id, complete: true,
}
const plainStyle = { fg: null, bg: null, bold: false, dim: false, italic: false, underline: false, strike: false }
const logStyles = [{ ...plainStyle, fg: 'red', bold: true }, { ...plainStyle, fg: 'green' }, { ...plainStyle, dim: true }]
const logSteps = [
  ['Set up job', 'success', [['Runner image: ubuntu-24.04'], ['Job is about to start running on runner acme-linux-4']]],
  ['Checkout', 'success', [['$ git fetch --depth=1 origin a8f3c21', 'command'], ['HEAD is now at a8f3c21 Validate checkout API contract']]],
  ['Install dependencies', 'success', [['$ npm ci', 'command'], ['added 812 packages in 14s']]],
  ['Run contract tests', 'failed', [
    ['$ npm run test:contract', 'command'],
    ['PASS  contracts/catalog.contract.test.ts', 'plain', [[0, 4, 1]]],
    ['PASS  contracts/cart.contract.test.ts', 'plain', [[0, 4, 1]]],
    ['FAIL  contracts/checkout.contract.test.ts', 'error', [[0, 4, 0]]],
    ['  POST /v2/checkout › returns the order total'],
    ['    Expected status 201, received 422', 'error'],
    ['    Response: { "error": "currency is required" }', 'plain', [[0, 47, 2]]],
    ['      at contracts/checkout.contract.test.ts:48:23', 'plain', [[0, 51, 2]]],
    ['Tests: 1 failed, 23 passed, 24 total', 'error'],
    ['Error: Process completed with exit code 1.', 'error'],
  ]],
]
const logLines = []
const logSections = logSteps.map(([title, status, lines], id) => {
  const start = logLines.length
  logLines.push(...lines.map(([text, kind = 'plain', spans]) => ({ text, kind, ...(spans ? { spans } : {}) })))
  return {
    id, parent: null, kind: 'step', node_id: logJob.id, title, status, duration_ms: (id + 1) * 9000, header_line: null,
    start_line: start, end_line: logLines.length - 1, error_count: lines.filter(([, kind]) => kind === 'error').length, open: status === 'failed',
  }
})
const errorLines = logLines.flatMap((line, index) => (line.kind === 'error' ? [index] : []))
const logDocument = {
  state: 'ready', node_id: logJob.id, revision: 1, complete: true, truncated: false, line_count: logLines.length,
  text_bytes: logLines.reduce((total, line) => total + line.text.length + 1, 0), first_timestamp_ms: now - 8 * 60_000,
  styles: logStyles, sections: logSections, error_lines: errorLines, warning_lines: [], error_total: errorLines.length,
  warning_total: 0, focus_line: errorLines[0], updated_at_ms: now - 4 * 60_000, stable_lines: logLines.length, previous_revision: null,
}
const metricSeries = {
  run_duration: [262, 248, 255, 291, 244, 238, 242],
  success_rate: [92, 100, 88, 75, 100, 96, 87.5],
  run_frequency: [11, 14, 9, 12, 16, 13, 8],
}
const aggregated = ({ metric_type, aggregation_period }) => ({
  metric_type, aggregation_period, total_count: 83,
  metrics: metricSeries[metric_type].map((value, index) => ({
    timestamp: new Date(now - (6 - index) * 86_400_000).toISOString(), value, count: metricSeries.run_frequency[index],
    min: metric_type === 'run_duration' ? value - 31 : null, max: metric_type === 'run_duration' ? value + 44 : null, avg: value,
  })),
})
export const fixedNow = now
const viewRules = { provider_ids: [], repositories: [], name_contains: null, statuses: [] }
const viewCapabilities = (pinned) => ({ rename: false, delete: false, edit_rules: false, edit_members: pinned, edit_display: true, reorder: pinned })
const pinnedPipelines = [release, pipelines[2]]
const viewPipeline = (pipeline) => ({ ...pipeline, organization: pipeline.repository.split('/')[0], pinned: pinnedPipelines.includes(pipeline), hidden: false, memberships: [] })
export const pipelineViews = [
  { id: 'all', kind: 'all', name: 'All pipelines', rules: viewRules, display: { sort: 'name', direction: 'asc', sections: 'repository', layout: 'table' }, capabilities: viewCapabilities(false), count: pipelines.length, parent: null },
  { id: 'pinned', kind: 'pinned', name: 'Pinned', rules: viewRules, display: { sort: 'manual', direction: 'asc', sections: 'none', layout: 'cards' }, capabilities: viewCapabilities(true), count: pinnedPipelines.length, parent: null },
]
const SEVERITY = ['failed', 'degraded', 'running', 'pending', 'cancelled', 'success', 'skipped']
const viewSection = (id, kind, title, members) => {
  const lead = [...members].sort((a, b) => SEVERITY.indexOf(a.status) - SEVERITY.indexOf(b.status))[0]
  return {
    id, kind, title, organization: kind === 'none' ? null : members[0].repository.split('/')[0],
    provider_ids: [...new Set(members.map((pipeline) => pipeline.provider_id))].sort((a, b) => a - b),
    status: lead?.status ?? null, status_pipeline_id: lead?.id ?? null, last_run: members.map((pipeline) => pipeline.last_run).sort().at(-1) ?? null,
    pending_fetch: false, pipelines: members.map(viewPipeline),
  }
}
function pipelinePage(viewId) {
  const pinned = viewId === 'pinned'
  const members = pinned ? pinnedPipelines : pipelines
  const sections = pinned
    ? [viewSection('all', 'none', '', members)]
    : [...new Set(members.map((pipeline) => pipeline.repository))]
        .map((repository) => viewSection(`repo:${repository}`, 'repository', repository.split('/').slice(1).join('/'), members.filter((pipeline) => pipeline.repository === repository)))
        .sort((a, b) => a.title.localeCompare(b.title))
  return {
    view: pipelineViews.find((view) => view.id === viewId) ?? pipelineViews[0],
    pinned: pinned ? [] : pinnedPipelines.map(viewPipeline),
    pinned_blocks: [],
    sections,
    total: sections.reduce((sum, section) => sum + section.pipelines.length, 0),
    hidden_count: 0,
    filters_active: false,
    facets: {
      organizations: [...new Set(members.map((pipeline) => pipeline.repository.split('/')[0]))].sort(),
      provider_ids: [...new Set(members.map((pipeline) => pipeline.provider_id))].sort((a, b) => a - b),
    },
  }
}
export function mockResponse(method, url, body = null) {
  const path = decodeURIComponent(url.pathname.replace('/api/v1', ''))
  // Metrics are queried with POST; that is the only non-GET request a capture may make.
  if (method === 'POST' && path === '/metrics/aggregated') return aggregated(body)
  if (method !== 'GET') throw new Error(`Unexpected mutation: ${method} ${path}`)
  if (path === '/setup/status') return { config_exists: true, config_valid: true, validation_errors: [], needs_setup: false, needs_migration: false }
  if (path === '/vault/status') return { is_unlocked: true, password_source: 'none', backend: 'sqlite', requires_password: false }
  if (path === '/providers') return providers
  if (path === '/plugins') return plugins
  if (path === '/pipeline-views') return pipelineViews
  if (path === '/pipeline-views/pins') return []
  if (path === '/pipeline-views/page') return pipelinePage(url.searchParams.get('view') ?? 'all')
  if (path === '/health') return { version: '0.4.0' }
  if (path === '/pipelines/cached') return pipelines.filter((pipeline) => !url.searchParams.has('provider_id') || pipeline.provider_id === Number(url.searchParams.get('provider_id')))
  if (/^\/providers\/[1-4]\/table-schema$/.test(path)) return tableSchema
  if (/^\/providers\/[1-4]\/features$/.test(path)) return []
  if (/^\/providers\/[1-4]\/permissions$/.test(path)) return { permissions: [], all_granted: true, checked_at: ago(1) }
  if (/^\/preferences\/table\/[1-4]\/pipeline_runs$/.test(path)) return null
  if (/^\/preferences\/table\/[1-4]\/pipeline_runs\/default$/.test(path)) return { columnOrder: tableSchema.tables[0].columns.map(({ id }) => id), columnVisibility: {} }
  if (path === '/metrics/config') return { enabled: true, default_retention_days: 30, updated_at: ago(1) }
  if (path === '/metrics/storage/stats') return { total_metrics_count: 83, estimated_size_bytes: 20_480, estimated_size_mb: 0.02, last_cleanup_at: null, updated_at: ago(1), by_pipeline: [] }
  if (path === '/storage/config') return {
    summary: 'SQLite',
    config: {
      general: { metrics_enabled: true, default_refresh_interval: 60, status_pages_enabled: true, proxy_url: '' },
      server: { bind_addr: '127.0.0.1:8080', cors_allow_all: false },
      storage: { data_dir: '/data', backend: 'sqlite' },
    },
  }
  for (const pipeline of pipelines) {
    if (path === `/pipelines/${pipeline.id}/workflow-params`) return parameters
    if (path === `/metrics/pipelines/${pipeline.id}/config`) return { pipeline_id: pipeline.id, enabled: true, retention_days: 30, created_at: ago(10_080), updated_at: ago(1) }
    const runs = runsFor(pipeline)
    if (path === `/pipelines/${pipeline.id}/runs`) return { runs, total_count: runs.length, has_more: false, is_complete: true, page: 1, page_size: 20, total_pages: 1 }
    const run = runs.find((item) => path === `/pipelines/${pipeline.id}/runs/${item.run_number}`)
    if (run) return run
    const logs = `/pipelines/${pipeline.id}/runs/${runs[0].run_number}/logs`
    if (path === logs) return logTree
    if (path === `${logs}/${logJob.id}`) return logDocument
    if (path === `${logs}/${logJob.id}/lines`) {
      const start = Number(url.searchParams.get('start'))
      return { start, lines: logLines.slice(start, start + Number(url.searchParams.get('count'))) }
    }
  }
  throw new Error(`Unexpected API request: ${method} ${path}`)
}
