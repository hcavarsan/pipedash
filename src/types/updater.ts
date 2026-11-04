export interface UpdateFailure {
  stage: 'check' | 'download' | 'storage' | 'install' | 'recovery'
  code: string
  message: string
}

export interface UpdateNotice {
  version: string
  outcome: 'updated' | 'incomplete'
}

export interface UpdateStatus {
  revision: number
  current_version: string
  automatic_updates: boolean
  mode: 'native' | 'external' | 'unsupported'
  install_source: string
  reason: string | null
  target: string | null
  phase: 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'preparing' | 'installing' | 'restarting'
  available_version: string | null
  release_url: string | null
  last_checked_at: string | null
  downloaded_bytes: number
  total_bytes: number | null
  can_check: boolean
  can_download: boolean
  can_install: boolean
  error: UpdateFailure | null
  notice: UpdateNotice | null
}
