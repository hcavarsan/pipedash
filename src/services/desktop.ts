import { invoke } from '@tauri-apps/api/core'

import { toPipedashError } from '../types/errors'
import type { UpdateStatus } from '../types/updater'

export const DESKTOP_UPDATE_EVENT = 'desktop-update-changed'
export const DESKTOP_UPDATE_RESET_EVENT = 'pipedash:desktop-update-reset'
export const DESKTOP_UPDATE_RECONNECT_EVENT = 'pipedash:desktop-update-reconnect'

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw toPipedashError(error)
  }
}

export const desktopService = {
  getUpdateStatus: (): Promise<UpdateStatus> => call('get_update_status'),
  setAutomaticUpdates: (enabled: boolean): Promise<UpdateStatus> => call('set_automatic_updates', { enabled }),
  checkForUpdates: (): Promise<UpdateStatus> => call('check_for_updates'),
  downloadUpdate: (): Promise<UpdateStatus> => call('download_update'),
  installUpdate: (version: string): Promise<void> => call('install_update', { version }),
  acknowledgeUpdateNotice: (version: string): Promise<UpdateStatus> => call('acknowledge_update_notice', { version }),
}
