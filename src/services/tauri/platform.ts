import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { platform } from '@tauri-apps/plugin-os'

import { isMobilePlatform } from '../../utils/platform'
import type { NativeOnlyService, PlatformService } from '../types'

export async function promptLogDestination(fileName: string): Promise<string | null | undefined> {
  if (isMobilePlatform()) {
    return null
  }

  return (await save({ defaultPath: fileName, filters: [{ name: 'Log', extensions: ['log'] }] })) ?? undefined
}

export const tauriPlatformService: PlatformService & NativeOnlyService = {
  openUrl: async (url) => {
    await openUrl(url, ['ios', 'android'].includes(platform()) ? 'inAppBrowser' : undefined)
  },

  bootstrapApp: async () => invoke<void>('bootstrap_app'),

  restartApp: async () => {
    await invoke<void>('restart_app')
  },

  setRefreshMode: async (mode) => invoke<void>('set_refresh_mode', { mode }),

  showSystemNotification: async (title, body, target) =>
    invoke<void>('show_system_notification', { title, body, target }),
}
