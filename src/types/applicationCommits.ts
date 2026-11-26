export type LocationState = 'live' | 'behind' | 'not_deployed'

export interface CommitLocation {
  application_id: number
  application_name: string
  target_label: string
  state: LocationState
  label: string
}

export interface CommitLocations {
  title: string
  entries: CommitLocation[]
}

export interface DeployCommit {
  sha: string
  short: string
  message: string | null
  authored_at: string | null
}

export interface DeployChanges {
  title: string
  commits: DeployCommit[]
  unavailable_reason: string | null
}
