import type { PipelineStatus } from './pipelines'

export interface ApplicationBoardQuery {
  search: string | null
}

export interface BoardPipeline {
  pipeline_id: string
  name: string
  repository: string
  status: PipelineStatus
}

export interface BoardTarget {
  target_key: string
  name: string
}

export interface BoardEnvironment {
  group_key: string
  label: string
  targets: BoardTarget[]
}

export interface BoardApplication {
  id: number
  name: string
  editable: boolean
  notice: string | null
  build: BoardPipeline[]
  deploy: BoardPipeline[]
  environments: BoardEnvironment[]
}

export interface BoardCopy {
  title: string
  help: string
  search_placeholder: string
  back: string
  unassigned: string
  unassigned_empty: string
  unassigned_drop: string
  applications: string
  applications_empty: string
  new_application_drop: string
  open_application: string
  build: string
  deploy: string
  lane_empty: string
  environments: string
  environments_empty: string
  new_environment_drop: string
  new_environment: string
  environment_name: string
  create_environment: string
  cancel: string
}

export interface ApplicationBoard {
  unassigned: BoardPipeline[]
  applications: BoardApplication[]
  copy: BoardCopy
}
