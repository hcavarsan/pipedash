import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface NotificationStoreState {
  failureNotificationsEnabled: boolean
}

interface NotificationStoreActions {
  setFailureNotificationsEnabled: (enabled: boolean) => void
}

type NotificationStore = NotificationStoreState & NotificationStoreActions

const initialState: NotificationStoreState = {
  failureNotificationsEnabled: false,
}

export const useNotificationStore = create<NotificationStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        setFailureNotificationsEnabled: (enabled) => set({ failureNotificationsEnabled: enabled }),
      }),
      {
        name: 'pipedash-notifications',
        partialize: (state) => ({
          failureNotificationsEnabled: state.failureNotificationsEnabled,
        }),
      }
    ),
    { name: 'NotificationStore' }
  )
)
