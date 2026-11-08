import { logger } from '../../../lib/logger'
import { service } from '../../../services/service'
import { useNotificationStore } from '../../../stores/notificationStore'
import type { PreferenceChange } from '../../../types/notification_preferences'
import type { WatchOnce } from '../../../types/notifications'
import { displayErrorNotification } from '../../../utils/errorDisplay'

async function applyChange(change: PreferenceChange): Promise<void> {
  const { preferences, applySnapshot } = useNotificationStore.getState()

  applySnapshot(await service.changeNotificationPreference(change, preferences))
}

export async function changePreference(change: PreferenceChange): Promise<void> {
  try {
    await applyChange(change)
  } catch (error) {
    displayErrorNotification(error, "Couldn't update notification settings")
  }
}

export async function refreshPreferenceStates(): Promise<void> {
  const { failed } = useNotificationStore.getState().preferences

  try {
    await applyChange({ type: 'kind', kind: 'failed', enabled: failed })
  } catch (error) {
    logger.warn('changePreference', "Couldn't refresh the notification states", error)
  }
}

export function recordFired(watches: WatchOnce[]): void {
  useNotificationStore.getState().markFired(watches)
  void refreshPreferenceStates()
}
