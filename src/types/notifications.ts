import type { HealthLevel } from './health'
import type { PipelineStatus } from './pipelines'

export interface BuildWatchSnapshot {
  sessionId: string
  pipelineId: string
  pipelineName: string
  providerName: string
  providerType: string
  runNumber: number
  branch: string | null
  status: PipelineStatus
  startedAt: number
  concludedAt: number | null
  updatedAt: number
  monitoring: 'watching' | 'completed' | 'interrupted' | 'stopped'
  url: string
}

export interface BuildWatchStatus {
  supported: boolean
  available: boolean
  reason: string | null
  watch: BuildWatchSnapshot | null
}

export interface AlertHostInfo {
  name: string
  push: boolean
  standalone: boolean
}

export interface LocalNotificationSettings {
  enabled: boolean
  available: boolean
  reason: string | null
  alertHost: AlertHostInfo | null
  pushProblem: string | null
}

export type AlertKind = 'failed' | 'degraded' | 'long_run' | 'outage' | 'recovered' | 'finished' | 'test'

export type PreferenceKind = Exclude<AlertKind, 'test' | 'finished'>

export interface WatchOnce {
  pipeline_id: string
  since: string
  fired_at?: string | null
}

export type NotificationPreferences = Record<PreferenceKind, boolean> & {
  muted_providers: number[]
  muted_pipelines: string[]
  watched_pipelines: string[]
  watch_once: WatchOnce[]
}

export interface RunFinish {
  at: string
  previous: string | null
}

export interface HostAlert {
  id: string
  seq: number
  key: string
  kind: AlertKind
  tone: CenterNotificationTone
  title: string
  body: string
  pipeline_id: string | null
  provider_id: number | null
  provider_ids: number[]
  created_at: string
  finish?: RunFinish | null
}

export interface PushRelaySettings {
  enabled: boolean
  relay_url: string | null
  from_env: boolean
  official_url: string
}

export interface AlertLink {
  url: string
  kind: 'incident' | 'status_page'
}

export interface ShownNotification {
  id: number
  at: string
  tone: CenterNotificationTone
  key: string
  title: string
  body: string
  kind: AlertKind
  pipeline_id: string | null
  provider_id: number | null
  provider_ids: number[]
  finish?: RunFinish
  level?: HealthLevel
  link?: AlertLink
  fired: WatchOnce | null
}

export interface MobileDevice {
  id: string
  device_name: string
  platform: 'ios'
  created_at: string
  last_success_at: string | null
}

export interface MobileDevicesResponse {
  relay_configured: boolean
  subscriptions: MobileDevice[]
}

export type CenterNotificationTone = 'danger' | 'warning' | 'success'

export interface CenterNotification {
  key: string
  at: number
  read: boolean
  tone: CenterNotificationTone
  level: HealthLevel | null
  title: string
  body: string
  providerIds: number[]
  pipelineId: string | null
  link: AlertLink | null
  runNumber?: number | null
}

export interface NotificationCenterSnapshot {
  revision: number
  items: CenterNotification[]
  unread: number
}

export type NotificationCenterChange =
  | { type: 'push'; notice: CenterNotification }
  | { type: 'insert_missed'; notice: CenterNotification }
  | { type: 'read'; key: string | null }
  | { type: 'clear' }
