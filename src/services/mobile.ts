import { invoke } from '@tauri-apps/api/core'

import type {
  AlertHostInfo,
  AppLockStatus,
  BuildWatchStatus,
  ExportFileResult,
  HostAlert,
  LocalNotificationSettings,
  NativeIntent,
  NotificationPreferences,
  UnlockVaultResponse,
  WatchOnce,
} from '../types/domain'
import { toPipedashError } from '../types/errors'

export const NATIVE_INTENT_EVENT = 'native-intent'
export const STATUS_BAR_TAP_EVENT = 'native-status-bar-tap'
export const NATIVE_ACCESSIBILITY_EVENT = 'pipedash:accessibility-change'

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (error) {
    throw toPipedashError(error)
  }
}

export type HapticKind = 'selection' | 'light' | 'medium' | 'heavy' | 'soft' | 'rigid' | 'success' | 'warning' | 'error'

export type NativeAppearance = 'light' | 'dark' | 'system'

export interface NativeAccessibility {
  reduceTransparency: boolean
  increaseContrast: boolean
}

export const mobileService = {
  unlockWithBiometrics: (): Promise<UnlockVaultResponse> => call('unlock_with_biometrics'),

  setAppLock: (enabled: boolean, lockOnBackground: boolean): Promise<AppLockStatus> =>
    call('set_app_lock', { enabled, lockOnBackground }),

  saveExportFile: (fileName: string, contents: string | Uint8Array<ArrayBuffer>): Promise<ExportFileResult> =>
    call('save_export_file', {
      fileName,
      contents: Array.from(typeof contents === 'string' ? new TextEncoder().encode(contents) : contents),
    }),

  shareLink: (url: string, sourceRect: DOMRect): Promise<boolean> =>
    call('native_share_link', {
      url,
      sourceRect: { x: sourceRect.x, y: sourceRect.y, width: sourceRect.width, height: sourceRect.height },
    }),

  getBuildWatch: (): Promise<BuildWatchStatus> => call('get_build_watch'),

  startBuildWatch: (pipelineId: string, runNumber: number): Promise<BuildWatchStatus> =>
    call('start_build_watch', { pipelineId, runNumber }),

  stopBuildWatch: (): Promise<BuildWatchStatus> => call('stop_build_watch'),

  getLocalNotificationSettings: (): Promise<LocalNotificationSettings> => call('get_local_notification_settings'),

  setLocalNotificationsEnabled: (enabled: boolean): Promise<LocalNotificationSettings> =>
    call('set_local_notifications_enabled', { enabled }),

  sendTestAlert: (): Promise<void> => call('send_test_alert'),

  pairAlertHost: (address: string, code: string | null, allowPrivateHttp: boolean): Promise<AlertHostInfo> =>
    call('pair_alert_host', { address, code, allowPrivateHttp }),

  removeAlertHost: (): Promise<void> => call('remove_alert_host'),

  takeMissedAlerts: (): Promise<HostAlert[]> => call('take_missed_alerts'),

  setNotificationPreferences: (preferences: NotificationPreferences): Promise<WatchOnce[]> =>
    call('set_notification_preferences', { preferences }),

  loadNotificationPreferences: (): Promise<NotificationPreferences | null> => call('load_notification_preferences'),

  haptic: (kind: HapticKind): Promise<void> => call('native_haptic', { kind }),

  setAppearance: (scheme: NativeAppearance): Promise<void> => call('native_set_appearance', { scheme }),

  accessibility: (): Promise<NativeAccessibility> => call('native_accessibility'),

  openAppSettings: (): Promise<void> => call('native_open_settings'),

  setBadge: (count: number): Promise<void> => call('native_set_badge', { count }),

  releasePrivacyCover: (): Promise<void> => call('native_release_privacy_cover'),

  takePendingIntent: (): Promise<NativeIntent | null> => call('native_take_pending_intent'),
}
