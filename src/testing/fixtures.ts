import type { Pipeline, PipelineView, PipelineViewPage, ProviderSummary, ViewPipeline } from '../types'

export function providerSummaryFixture(overrides: Partial<ProviderSummary> = {}): ProviderSummary {
  return {
    id: 1,
    name: 'Provider',
    provider_type: 'github',
    icon: null,
    pipeline_count: 0,
    last_updated: null,
    refresh_interval: 30,
    configured_repositories: [],
    last_fetch_status: 'never',
    last_fetch_error: null,
    last_fetch_error_kind: null,
    last_fetch_at: null,
    last_success_at: null,
    paused_until: null,
    freshness: { state: 'never', stale_at: null },
    recovery: null,
    health: { level: 'operational', signals: [], next_attempt_at: null, in_flight: false, status_page: null },
    ...overrides,
  }
}

export function pipelineFixture(overrides: Partial<Pipeline> = {}): Pipeline {
  return {
    id: 'pipeline-1',
    provider_id: 1,
    provider_type: 'github',
    name: 'Pipeline',
    status: 'success',
    last_run: null,
    last_updated: '2026-01-01T00:00:00Z',
    repository: 'org/repo',
    branch: null,
    workflow_file: null,
    ...overrides,
  }
}

export function viewPipelineFixture(overrides: Partial<ViewPipeline> = {}): ViewPipeline {
  return {
    ...pipelineFixture(),
    organization: 'org',
    pinned: false,
    hidden: false,
    memberships: [],
    ...overrides,
  }
}

export function pipelineViewFixture(overrides: Partial<PipelineView> = {}): PipelineView {
  return {
    id: 'all',
    kind: 'all',
    name: 'All',
    rules: { provider_ids: [], repositories: [], name_contains: null, statuses: [] },
    display: { sort: 'name', direction: 'asc', sections: 'none', layout: 'table' },
    capabilities: {
      rename: false,
      delete: false,
      edit_rules: false,
      edit_members: false,
      edit_display: true,
      reorder: false,
    },
    count: 0,
    parent: null,
    ...overrides,
  }
}

export function pipelineViewPageFixture(overrides: Partial<PipelineViewPage> = {}): PipelineViewPage {
  return {
    view: pipelineViewFixture(),
    pinned: [],
    pinned_blocks: [],
    sections: [],
    total: 0,
    hidden_count: 0,
    filters_active: false,
    facets: { organizations: [], provider_ids: [] },
    ...overrides,
  }
}
