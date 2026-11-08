import type {
  LegacyImportResult,
  PinnedItem,
  PipelineNavigation,
  PipelineView,
  PipelineViewPage,
  ViewEditorOptions,
  ViewMember,
  ViewSelectionEntry,
} from '../../types/views'
import type { HttpClient } from '../http'
import type { ViewsService } from '../types'

export function createViewsApi(client: HttpClient): ViewsService {
  return {
    getTablePreferences: async (providerId, tableId) =>
      client.get<string | null>(`/preferences/table/${providerId}/${encodeURIComponent(tableId)}`),

    saveTablePreferences: async (providerId, tableId, preferencesJson) => {
      await client.requestVoid('PUT', `/preferences/table/${providerId}/${encodeURIComponent(tableId)}`, {
        preferences_json: preferencesJson,
      })
    },

    getDefaultTablePreferences: async (providerId, tableId) => {
      const defaults = await client.get<unknown>(
        `/preferences/table/${providerId}/${encodeURIComponent(tableId)}/default`
      )

      return JSON.stringify(defaults)
    },

    listPipelineViews: async () => client.get<PipelineView[]>('/pipeline-views'),

    getPipelineViewPage: async (query) => {
      const params = new URLSearchParams()

      for (const [key, value] of Object.entries(query)) {
        if (value !== undefined && value !== null) {
          params.set(key, String(value))
        }
      }

      return client.get<PipelineViewPage>(`/pipeline-views/page?${params}`)
    },

    createPipelineView: async (input) => client.post<PipelineView>('/pipeline-views', input),

    updatePipelineView: async (id, patch) =>
      client.put<PipelineView>(`/pipeline-views/${encodeURIComponent(id)}`, patch),

    deletePipelineView: async (id) => {
      await client.requestVoid('DELETE', `/pipeline-views/${encodeURIComponent(id)}`)
    },

    reorderPipelineViews: async (order) => {
      await client.requestVoid('PUT', '/pipeline-views/order', { order })
    },

    listPipelineViewMembers: async (id) =>
      client.get<ViewMember[]>(`/pipeline-views/${encodeURIComponent(id)}/members`),

    setPipelineViewMembership: async (id, pipelineId, member) => {
      await client.requestVoid('PUT', `/pipeline-views/${encodeURIComponent(id)}/membership`, {
        pipeline_id: pipelineId,
        member,
      })
    },

    setPipelineHidden: async (pipelineId, hidden) => {
      await client.requestVoid('PUT', '/pipeline-views/hidden', { pipeline_id: pipelineId, hidden })
    },

    listPins: async () => client.get<PinnedItem[]>('/pipeline-views/pins'),

    setPin: async (kind, ref, pinned) => {
      await client.requestVoid('PUT', '/pipeline-views/pins', { kind, ref, pinned })
    },

    importLegacyPipelineMarks: async (marks) => client.post<LegacyImportResult>('/pipeline-views/legacy-import', marks),

    getViewSelection: async (pipelineIds) =>
      client.post<ViewSelectionEntry[]>('/pipeline-views/selection', { pipeline_ids: pipelineIds }),

    setViewMemberships: async (id, change) =>
      client.put<ViewSelectionEntry[]>(`/pipeline-views/${encodeURIComponent(id)}/memberships`, change),

    getViewEditorOptions: async (view) =>
      client.get<ViewEditorOptions>(
        `/pipeline-views/editor-options${view === null ? '' : `?${new URLSearchParams({ view })}`}`
      ),

    getPipelineNavigation: async (search) =>
      client.get<PipelineNavigation>(
        `/pipeline-views/navigation${search ? `?${new URLSearchParams({ search })}` : ''}`
      ),
  }
}
