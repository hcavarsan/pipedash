import type { PipelineViewQuery } from '../types/domain'
import type { LogSearchQuery } from '../types/logs'

export const queryKeys = {
  providers: {
    all: ['providers'] as const,
    list: () => [...queryKeys.providers.all, 'list'] as const,
    detail: (id: number) => [...queryKeys.providers.all, 'detail', id] as const,
    features: (id: number) => [...queryKeys.providers.all, 'features', id] as const,
    permissions: (id: number) => [...queryKeys.providers.all, 'permissions', id] as const,
    schema: (id: number) => [...queryKeys.providers.all, 'schema', id] as const,
    refreshInterval: (id: number) => [...queryKeys.providers.all, 'refreshInterval', id] as const,
  },

  signIn: {
    status: (id: string) => ['signIn', 'status', id] as const,
  },

  pipelines: {
    all: ['pipelines'] as const,
    list: (filters?: { providerId?: number | undefined }) =>
      [...queryKeys.pipelines.all, 'list', filters ?? {}] as const,
    detail: (id: string) => [...queryKeys.pipelines.all, 'detail', id] as const,
    views: () => [...queryKeys.pipelines.all, 'views'] as const,
    pages: () => [...queryKeys.pipelines.all, 'page'] as const,
    page: (q: PipelineViewQuery) => [...queryKeys.pipelines.all, 'page', q] as const,
    viewMembers: (id: string) => [...queryKeys.pipelines.all, 'viewMembers', id] as const,
    pins: () => [...queryKeys.pipelines.all, 'pins'] as const,
  },

  runs: {
    all: ['runs'] as const,
    list: (pipelineId: string, page?: number, pageSize?: number) =>
      page === undefined
        ? ([...queryKeys.runs.all, 'list', pipelineId] as const)
        : ([...queryKeys.runs.all, 'list', pipelineId, { page, pageSize }] as const),
    infinite: (pipelineId: string, pageSize: number) =>
      [...queryKeys.runs.all, 'list', pipelineId, 'infinite', { pageSize }] as const,
    detail: (pipelineId: string, runNumber: number) =>
      [...queryKeys.runs.all, 'detail', pipelineId, runNumber] as const,
    active: (pipelineId: string) => [...queryKeys.runs.all, 'active', pipelineId] as const,
    unfinished: (pipelineIds: string[]) => [...queryKeys.runs.all, 'unfinished', pipelineIds] as const,
  },

  metrics: {
    all: ['metrics'] as const,
    globalConfig: () => [...queryKeys.metrics.all, 'config', 'global'] as const,
    pipelineConfig: (pipelineId: string) => [...queryKeys.metrics.all, 'config', 'pipeline', pipelineId] as const,
    aggregated: (params: { metricType: string; aggregationPeriod: string; pipelineId?: string | undefined }) =>
      [...queryKeys.metrics.all, 'aggregated', params] as const,
    stats: () => [...queryKeys.metrics.all, 'stats'] as const,
  },

  plugins: {
    all: ['plugins'] as const,
    list: () => [...queryKeys.plugins.all, 'list'] as const,
  },

  setup: {
    all: ['setup'] as const,
    status: () => [...queryKeys.setup.all, 'status'] as const,
  },

  tableSchema: {
    all: ['tableSchema'] as const,
    detail: (providerId: number) => [...queryKeys.tableSchema.all, providerId] as const,
  },

  storage: {
    all: ['storage'] as const,
    config: () => [...queryKeys.storage.all, 'config'] as const,
    paths: () => [...queryKeys.storage.all, 'paths'] as const,
    configContent: () => [...queryKeys.storage.all, 'configContent'] as const,
    defaultDataDir: () => [...queryKeys.storage.all, 'defaultDataDir'] as const,
    analysis: (content: string) => [...queryKeys.storage.all, 'analysis', content] as const,
  },

  cache: {
    all: ['cache'] as const,
    stats: () => [...queryKeys.cache.all, 'stats'] as const,
  },

  runLogs: {
    all: ['runLogs'] as const,
    tree: (pipelineId: string, runNumber: number) => [...queryKeys.runLogs.all, 'tree', pipelineId, runNumber] as const,
    doc: (pipelineId: string, runNumber: number, nodeId: string) =>
      [...queryKeys.runLogs.all, 'doc', pipelineId, runNumber, nodeId] as const,
    search: (pipelineId: string, runNumber: number, nodeId: string, revision: number, query: LogSearchQuery) =>
      [...queryKeys.runLogs.all, 'search', pipelineId, runNumber, nodeId, revision, query] as const,
  },

  workflows: {
    all: ['workflows'] as const,
    parameters: (providerId: number, pipelineId: string) =>
      [...queryKeys.workflows.all, 'parameters', providerId, pipelineId] as const,
  },

  vault: {
    all: ['vault'] as const,
    passwordStatus: () => [...queryKeys.vault.all, 'passwordStatus'] as const,
    status: () => [...queryKeys.vault.all, 'status'] as const,
  },

  app: {
    version: () => ['app', 'version'] as const,
  },

  push: {
    all: ['push'] as const,
    status: () => [...queryKeys.push.all, 'status'] as const,
    devices: () => [...queryKeys.push.all, 'devices'] as const,
  },

  buildWatch: {
    all: ['buildWatch'] as const,
    status: () => [...queryKeys.buildWatch.all, 'status'] as const,
  },

  updates: {
    all: ['updates'] as const,
    status: () => [...queryKeys.updates.all, 'status'] as const,
    connection: () => [...queryKeys.updates.all, 'connection'] as const,
  },

  localNotifications: {
    all: ['localNotifications'] as const,
    settings: () => [...queryKeys.localNotifications.all, 'settings'] as const,
  },

  tablePreferences: {
    all: ['tablePreferences'] as const,
    detail: (providerId: number, tableId: string) => [...queryKeys.tablePreferences.all, providerId, tableId] as const,
  },

  platform: {
    all: ['platform'] as const,
    current: () => [...queryKeys.platform.all, 'current'] as const,
  },
} as const
