import { apiService } from './api'
import { hasLocalCore } from './connection'
import { tauriService } from './tauri'
import type { PipedashService } from './types'

export const service: PipedashService = hasLocalCore() ? tauriService : apiService
