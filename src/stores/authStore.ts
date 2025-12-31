import { create } from 'zustand'
import { devtools, subscribeWithSelector } from 'zustand/middleware'

import { notifications } from '@mantine/notifications'

const STORAGE_KEY = 'pipedash_vault_password'
const FAILURE_THRESHOLD = 3
const NOTIFICATION_DEBOUNCE_MS = 500

interface AuthStoreState {
  token: string | null
  isAuthenticated: boolean
  returnPath: string | null
  consecutiveFailures: number
  lastNotificationTime: number | null
}

interface AuthStoreActions {
  setToken: (token: string) => void
  clearToken: () => void
  setReturnPath: (path: string | null) => void
  consumeReturnPath: () => string | null
  incrementFailure: () => boolean
  resetFailures: () => void
}

type AuthStore = AuthStoreState & AuthStoreActions

const getInitialToken = (): string | null => {
  return localStorage.getItem(STORAGE_KEY) || import.meta.env.VITE_API_TOKEN || null
}

const initialState: AuthStoreState = {
  token: getInitialToken(),
  isAuthenticated: !!getInitialToken(),
  returnPath: null,
  consecutiveFailures: 0,
  lastNotificationTime: null,
}

export const useAuthStore = create<AuthStore>()(
  devtools(
    subscribeWithSelector((set, get) => ({
      ...initialState,

      setToken: (token) => {
        localStorage.setItem(STORAGE_KEY, token)
        set({
          token,
          isAuthenticated: true,
          consecutiveFailures: 0,
        })
      },

      clearToken: () => {
        localStorage.removeItem(STORAGE_KEY)
        set({
          token: null,
          isAuthenticated: false,
          consecutiveFailures: 0,
        })
      },

      setReturnPath: (path) => set({ returnPath: path }),

      consumeReturnPath: () => {
        const { returnPath } = get()


        set({ returnPath: null })
        
return returnPath
      },

      incrementFailure: () => {
        const { consecutiveFailures, lastNotificationTime } = get()
        const newCount = consecutiveFailures + 1
        const now = Date.now()

        if (newCount >= FAILURE_THRESHOLD) {
          const shouldNotify =
            !lastNotificationTime || now - lastNotificationTime > NOTIFICATION_DEBOUNCE_MS

          if (shouldNotify) {
            notifications.show({
              title: 'Session Expired',
              message: 'Please unlock the vault to continue.',
              color: 'red',
              autoClose: 5000,
            })
            set({ lastNotificationTime: now })
          }

          get().clearToken()
          
return true
        }

        set({ consecutiveFailures: newCount })
        
return false
      },

      resetFailures: () => set({ consecutiveFailures: 0 }),
    })),
    { name: 'AuthStore' }
  )
)

// Export getter for use outside React components (e.g., in api.ts)
export const getToken = () => useAuthStore.getState().token
