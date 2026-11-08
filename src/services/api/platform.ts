import { isRemoteClient } from '../connection'
import type { HttpClient } from '../http'
import { tauriPlatformService } from '../tauri/platform'
import type { PlatformService, ServerVersionService } from '../types'

export function createPlatformApi(client: HttpClient): PlatformService & ServerVersionService {
  return {
    openUrl: async (url) => {
      if (isRemoteClient()) {
        return tauriPlatformService.openUrl(url)
      }

      window.open(url, '_blank', 'noopener,noreferrer')
    },

    bootstrapApp: async () => undefined,

    restartApp: async () => {
      window.location.reload()
    },

    getServerVersion: async () => {
      const health = await client.get<{ version: string }>('/health')

      return health.version
    },
  }
}
