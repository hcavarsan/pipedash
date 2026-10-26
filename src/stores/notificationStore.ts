import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface NotificationStoreState {
  notificationsEnabled: boolean
}

interface NotificationStoreActions {
  setNotificationsEnabled: (enabled: boolean) => void
}

type NotificationStore = NotificationStoreState & NotificationStoreActions

const initialState: NotificationStoreState = {
  notificationsEnabled: false,
}

export function migrateNotificationSettings(persisted: unknown, version: number): NotificationStoreState {
  const saved = (persisted ?? {}) as Record<string, unknown>

  if (version >= 1) {
    return { notificationsEnabled: saved.notificationsEnabled === true }
  }

  return { notificationsEnabled: saved.failureNotificationsEnabled === true || saved.providerAlertsEnabled === true }
}

export const useNotificationStore = create<NotificationStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        setNotificationsEnabled: (enabled) => set({ notificationsEnabled: enabled }),
      }),
      {
        name: 'pipedash-notifications',
        version: 1,
        migrate: migrateNotificationSettings,
        partialize: (state) => ({ notificationsEnabled: state.notificationsEnabled }),
      }
    ),
    { name: 'NotificationStore' }
  )
)
