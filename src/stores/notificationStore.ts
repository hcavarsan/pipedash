import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface NotificationStoreState {
  failureNotificationsEnabled: boolean
  providerAlertsEnabled: boolean
}

interface NotificationStoreActions {
  setFailureNotificationsEnabled: (enabled: boolean) => void
  setProviderAlertsEnabled: (enabled: boolean) => void
}

type NotificationStore = NotificationStoreState & NotificationStoreActions

const initialState: NotificationStoreState = {
  failureNotificationsEnabled: false,
  providerAlertsEnabled: false,
}

export const useNotificationStore = create<NotificationStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        setFailureNotificationsEnabled: (enabled) => set({ failureNotificationsEnabled: enabled }),
        setProviderAlertsEnabled: (enabled) => set({ providerAlertsEnabled: enabled }),
      }),
      {
        name: 'pipedash-notifications',
        partialize: (state) => ({
          failureNotificationsEnabled: state.failureNotificationsEnabled,
          providerAlertsEnabled: state.providerAlertsEnabled,
        }),
      }
    ),
    { name: 'NotificationStore' }
  )
)
