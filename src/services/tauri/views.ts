import { invoke } from '@tauri-apps/api/core'

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
import type { ViewsService } from '../types'

export const viewsTauriService: ViewsService = {
  getTablePreferences: async (providerId, tableId) =>
    invoke<string | null>('get_table_preferences', { providerId, tableId }),

  saveTablePreferences: async (providerId, tableId, preferencesJson) =>
    invoke<void>('save_table_preferences', { providerId, tableId, preferencesJson }),

  getDefaultTablePreferences: async (providerId, tableId) =>
    invoke<string>('get_default_table_preferences', { providerId, tableId }),

  listPipelineViews: async () => invoke<PipelineView[]>('list_pipeline_views'),

  getPipelineViewPage: async (query) => invoke<PipelineViewPage>('get_pipeline_view_page', { query }),

  createPipelineView: async (input) => invoke<PipelineView>('create_pipeline_view', { input }),

  updatePipelineView: async (id, patch) => invoke<PipelineView>('update_pipeline_view', { id, patch }),

  deletePipelineView: async (id) => invoke<void>('delete_pipeline_view', { id }),

  reorderPipelineViews: async (order) => invoke<void>('reorder_pipeline_views', { order }),

  listPipelineViewMembers: async (id) => invoke<ViewMember[]>('list_pipeline_view_members', { id }),

  setPipelineViewMembership: async (id, pipelineId, member) =>
    invoke<void>('set_pipeline_view_membership', { id, pipelineId, member }),

  setPipelineHidden: async (pipelineId, hidden) => invoke<void>('set_pipeline_hidden', { pipelineId, hidden }),

  listPins: async () => invoke<PinnedItem[]>('list_pins'),

  setPin: async (kind, ref, pinned) => invoke<void>('set_pin', { kind, ref, pinned }),

  importLegacyPipelineMarks: async (marks) => invoke<LegacyImportResult>('import_legacy_pipeline_marks', { marks }),

  getViewSelection: async (pipelineIds) => invoke<ViewSelectionEntry[]>('get_view_selection', { pipelineIds }),

  setViewMemberships: async (id, change) => invoke<ViewSelectionEntry[]>('set_view_memberships', { id, change }),

  getViewEditorOptions: async (view) => invoke<ViewEditorOptions>('get_view_editor_options', { view }),

  getPipelineNavigation: async (search) =>
    invoke<PipelineNavigation>('get_pipeline_navigation', { search: search ?? null }),
}
