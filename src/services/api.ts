import { createApplicationsApi } from './api/applications'
import { createDevicesApi } from './api/devices'
import { createLogsApi } from './api/logs'
import { createMetricsApi } from './api/metrics'
import { createNotificationsApi } from './api/notifications'
import { createPlatformApi } from './api/platform'
import { createProvidersApi } from './api/providers'
import { createRunsApi } from './api/runs'
import { createStorageApi } from './api/storage'
import { createViewsApi } from './api/views'
import { HttpClient } from './http'
import type { PipedashService, PushService, ServerVersionService } from './types'

const client = new HttpClient()

export const apiService: PipedashService & ServerVersionService & PushService = {
  ...createProvidersApi(client),
  ...createRunsApi(client),
  ...createLogsApi(client),
  ...createMetricsApi(client),
  ...createViewsApi(client),
  ...createApplicationsApi(client),
  ...createStorageApi(client),
  ...createDevicesApi(client),
  ...createNotificationsApi(client),
  ...createPlatformApi(client),
}
