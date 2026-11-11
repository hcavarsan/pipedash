import { invoke } from '@tauri-apps/api/core'

import type {
  ApplicationDetail,
  ApplicationList,
  ApplicationMetrics,
  ApplicationRef,
  ChangesPage,
  CommitLocations,
  DeployChanges,
  Suggestion,
} from '../../types/applications'
import type { ApplicationsService } from '../types'

export const applicationsTauriService: ApplicationsService = {
  listApplications: async (query) => invoke<ApplicationList>('list_applications', { query }),

  getApplication: async (id) => invoke<ApplicationDetail>('get_application', { id }),

  getApplicationChanges: async (id, cursor, limit) =>
    invoke<ChangesPage>('get_application_changes', { id, cursor, limit }),

  getApplicationMetrics: async (query) => invoke<ApplicationMetrics>('get_application_metrics', { query }),

  createApplication: async (input) => invoke<ApplicationDetail>('create_application', { input }),

  editApplication: async (id, edit) => invoke<ApplicationDetail>('edit_application', { id, edit }),

  deleteApplication: async (id) => invoke<void>('delete_application', { id }),

  revertApplicationOverride: async (id, overrideId) =>
    invoke<ApplicationDetail>('revert_application_override', { id, overrideId }),

  listApplicationSuggestions: async () => invoke<Suggestion[]>('list_application_suggestions'),

  acceptApplicationSuggestions: async (ids) => invoke<void>('accept_application_suggestions', { ids }),

  getPipelineApplications: async (pipelineId) => invoke<ApplicationRef[]>('get_pipeline_applications', { pipelineId }),

  getCommitLocations: async (pipelineId, runNumber) =>
    invoke<CommitLocations>('get_commit_locations', { pipelineId, runNumber }),

  getDeployChanges: async (pipelineId, runNumber) =>
    invoke<DeployChanges>('get_deploy_changes', { pipelineId, runNumber }),
}
