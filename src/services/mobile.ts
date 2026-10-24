import { invoke } from '@tauri-apps/api/core'

import {
  type AppLockStatus,
  type ExportFileResult,
  type PushStatus,
  toPipedashError,
  type UnlockVaultResponse,
} from '../types'

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (error) {
    throw toPipedashError(error)
  }
}

export const mobileService = {
  unlockWithBiometrics: (): Promise<UnlockVaultResponse> => call('unlock_with_biometrics'),

  setAppLock: (enabled: boolean, lockOnBackground: boolean): Promise<AppLockStatus> =>
    call('set_app_lock', { enabled, lockOnBackground }),

  saveExportFile: (fileName: string, contents: string): Promise<ExportFileResult> =>
    call('save_export_file', { fileName, contents }),

  getPushStatus: (): Promise<PushStatus> => call('get_push_status'),

  pushPairServer: (serverUrl: string, apiToken: string, label?: string): Promise<PushStatus> =>
    call('push_pair_server', { serverUrl, apiToken, label }),

  pushUnpairServer: (id: string): Promise<PushStatus> => call('push_unpair_server', { id }),

  pushSendTest: (id: string): Promise<void> => call('push_send_test', { id }),
}
