import type { PipelineStatus } from './pipelines'

export interface ApplicationBoardQuery {
  search: string | null
}

export interface BoardCardRef {
  kind: 'pipeline' | 'target'
  key: string
  application_id: number | null
}

export interface BoardCard {
  item: BoardCardRef
  zone_id: string
  drop_class: string
  draggable: boolean
}

export interface BoardZoneRef {
  kind: 'unassigned' | 'new_application' | 'lane' | 'environment' | 'new_environment'
  application_id: number | null
  key: string | null
}

export interface BoardZone {
  id: string
  target: BoardZoneRef
  drop_class: string
  enabled: boolean
  label: string
  hint: string | null
}

export interface BoardPipeline {
  card: BoardCard
  name: string
  repository: string
  status: PipelineStatus
}

export interface BoardTarget {
  card: BoardCard
  name: string
}

export interface BoardLane {
  zone: BoardZone
  title: string
  pipelines: BoardPipeline[]
}

export interface BoardEnvironment {
  zone: BoardZone
  label: string
  targets: BoardTarget[]
}

export interface BoardApplication {
  id: number
  name: string
  notice: string | null
  build: BoardLane
  deploy: BoardLane
  environments: BoardEnvironment[]
  new_environment: BoardZone | null
}

export interface BoardCopy {
  title: string
  help: string
  search_placeholder: string
  back: string
  applications: string
  applications_empty: string
  open_application: string
  environments: string
  environments_empty: string
  new_environment: string
  environment_name: string
  create_environment: string
  cancel: string
}

export interface ApplicationBoard {
  unassigned: BoardLane
  new_application: BoardZone
  applications: BoardApplication[]
  copy: BoardCopy
}

export interface BoardDrop {
  card: BoardCardRef
  zone: BoardZoneRef
}

export type BoardDropOutcome =
  | { type: 'applied' }
  | { type: 'name_application'; pipeline_id: string }
  | { type: 'name_environment'; application_id: number; target_key: string }
