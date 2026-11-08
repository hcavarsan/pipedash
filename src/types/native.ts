export interface ExportFileResult {
  path: string
  shared: boolean
}

export interface NotificationTarget {
  key: string
  pipelineId?: string | undefined
  providerId?: number | undefined
  runNumber?: number | undefined
}

export type NativeUiAction = 'managePins' | 'manageViews'

export type NativeIntent =
  | ({ type: 'notification'; action: string } & Partial<NotificationTarget>)
  | { type: 'link'; path: string; runNumber?: number }
  | { type: 'ui'; action: NativeUiAction }
  | { type: 'pair'; link: string }

export interface PendingNativeIntent {
  id: number
  intent: NativeIntent
}

export interface DesktopSettings {
  show_tray: boolean
  keep_running_on_close: boolean
  notifications_enabled: boolean
}

export type DesktopSettingsPatch = Partial<DesktopSettings>

export interface DesktopStatus {
  revision: number
  settings: DesktopSettings
  tray_state: 'off' | 'ready' | 'error'
  tray_status: 'offline' | 'failing' | 'running' | 'attention' | 'idle'
  unread: number
  ready: boolean
  notification_preference_imported: boolean
  error: string | null
}

export type DevicePlatform = 'macos' | 'windows' | 'linux' | 'ios' | 'android'

export interface PairedDevice {
  id: string
  name: string
  platform: DevicePlatform
  created_at: string
  last_seen_at: string | null
  current: boolean
  push: boolean
  removable: boolean
}

export interface PairingCode {
  code: string
  link: string
  server_url: string
  expires_at: string
  loopback: boolean
  qr_svg: string
}

export interface RemoteResponse {
  status: number
  body: string
}

export interface SharingStatus {
  enabled: boolean
  running: boolean
  url: string | null
  error: string | null
  errorCode: 'PORT_IN_USE' | 'NO_NETWORK' | null
}
