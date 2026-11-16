import { applicationsTauriService } from './tauri/applications'
import { devicesTauriService } from './tauri/devices'
import { logsTauriService } from './tauri/logs'
import { metricsTauriService } from './tauri/metrics'
import { notificationsTauriService } from './tauri/notifications'
import { tauriPlatformService } from './tauri/platform'
import { providersTauriService } from './tauri/providers'
import { runsTauriService } from './tauri/runs'
import { storageTauriService } from './tauri/storage'
import { viewsTauriService } from './tauri/views'
import type { NativeOnlyService, PipedashService } from './types'

export const tauriService: PipedashService & NativeOnlyService = {
  ...providersTauriService,
  ...runsTauriService,
  ...logsTauriService,
  ...metricsTauriService,
  ...viewsTauriService,
  ...applicationsTauriService,
  ...storageTauriService,
  ...devicesTauriService,
  ...notificationsTauriService,
  ...tauriPlatformService,
}
