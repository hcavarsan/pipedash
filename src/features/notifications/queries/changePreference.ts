import { logger } from '../../../lib/logger'
import { service } from '../../../services/service'
import { useNotificationStore } from '../../../stores/notificationStore'
import type { PreferenceChange } from '../../../types/notification_preferences'
import type { WatchOnce } from '../../../types/notifications'
import { displayErrorNotification } from '../../../utils/errorDisplay'

let queue: Promise<unknown> = Promise.resolve()

function enqueue<T>(task: () => Promise<T>): Promise<T> {
  const run = queue.then(task)
  queue = run.catch(() => undefined)

  return run
}

async function applyChange(change: PreferenceChange): Promise<void> {
  const { preferences, applySnapshot } = useNotificationStore.getState()

  applySnapshot(await service.changeNotificationPreference(change, preferences))
}

export async function changePreference(change: PreferenceChange): Promise<boolean> {
  try {
    await enqueue(() => applyChange(change))

    return true
  } catch (error) {
    displayErrorNotification(error, "Couldn't update notification settings")

    return false
  }
}

export async function refreshPreferenceStates(): Promise<void> {
  try {
    await enqueue(() =>
      applyChange({ type: 'kind', kind: 'failed', enabled: useNotificationStore.getState().preferences.failed })
    )
  } catch (error) {
    logger.warn('changePreference', "Couldn't refresh the notification states", error)
  }
}

export function recordFired(watches: WatchOnce[]): void {
  useNotificationStore.getState().markFired(watches)
  void refreshPreferenceStates()
}
