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
import type { HttpClient } from '../http'
import type { ApplicationsService } from '../types'

function encodeQuery(query: object): string {
  const params = new URLSearchParams()

  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === null || (Array.isArray(value) && value.length === 0)) {
      continue
    }

    params.set(key, String(value))
  }

  return params.toString()
}

const applicationPath = (id: number): string => `/applications/${id}`

export function createApplicationsApi(client: HttpClient): ApplicationsService {
  return {
    listApplications: async (query) => client.get<ApplicationList>(`/applications?${encodeQuery(query)}`),

    getApplication: async (id) => client.get<ApplicationDetail>(applicationPath(id)),

    getApplicationChanges: async (id, cursor, limit) =>
      client.get<ChangesPage>(`${applicationPath(id)}/changes?${encodeQuery({ cursor, limit })}`),

    getApplicationMetrics: async (query) =>
      client.get<ApplicationMetrics>(`${applicationPath(query.application_id)}/metrics?${encodeQuery(query)}`),

    createApplication: async (input) => client.post<ApplicationDetail>('/applications', input),

    editApplication: async (id, edit) => client.post<ApplicationDetail>(`${applicationPath(id)}/edits`, edit),

    deleteApplication: async (id) => {
      await client.requestVoid('DELETE', applicationPath(id))
    },

    revertApplicationOverride: async (id, overrideId) =>
      client.delete<ApplicationDetail>(`${applicationPath(id)}/overrides/${overrideId}`),

    listApplicationSuggestions: async () => client.get<Suggestion[]>('/applications/suggestions'),

    acceptApplicationSuggestions: async (ids) => {
      await client.requestVoid('POST', '/applications/suggestions/accept', { ids })
    },

    getPipelineApplications: async (pipelineId) =>
      client.get<ApplicationRef[]>(`/applications/by-pipeline/${encodeURIComponent(pipelineId)}`),

    getCommitLocations: async (pipelineId, runNumber) =>
      client.get<CommitLocations>(
        `/applications/commit-locations?${encodeQuery({ pipeline_id: pipelineId, run_number: runNumber })}`
      ),

    getDeployChanges: async (pipelineId, runNumber) =>
      client.get<DeployChanges>(
        `/applications/deploy-changes?${encodeQuery({ pipeline_id: pipelineId, run_number: runNumber })}`
      ),
  }
}
