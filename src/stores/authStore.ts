import { create } from 'zustand'
import { devtools, subscribeWithSelector } from 'zustand/middleware'
import { notifications } from '../lib/notifications'

const STORAGE_KEY = 'pipedash_vault_password'
const FAILURE_THRESHOLD = 3
const NOTIFICATION_DEBOUNCE_MS = 500

interface AuthStoreState {
  token: string | null
  consecutiveFailures: number
  lastNotificationTime: number | null
}

interface AuthStoreActions {
  setToken: (token: string) => void
  clearToken: () => void
  incrementFailure: () => boolean
  resetFailures: () => void
}

type AuthStore = AuthStoreState & AuthStoreActions

const readStoredToken = (): string | null => {
  try {
    return localStorage.getItem(STORAGE_KEY)
  } catch {
    return null
  }
}

const writeStoredToken = (token: string | null): void => {
  try {
    if (token === null) {
      localStorage.removeItem(STORAGE_KEY)
    } else {
      localStorage.setItem(STORAGE_KEY, token)
    }
  } catch {
    return
  }
}

const initialState: AuthStoreState = {
  token: readStoredToken() || import.meta.env.VITE_API_TOKEN || null,
  consecutiveFailures: 0,
  lastNotificationTime: null,
}

export const useAuthStore = create<AuthStore>()(
  devtools(
    subscribeWithSelector((set, get) => ({
      ...initialState,

      setToken: (token) => {
        writeStoredToken(token)
        set({
          token,
          consecutiveFailures: 0,
        })
      },

      clearToken: () => {
        writeStoredToken(null)
        set({
          token: null,
          consecutiveFailures: 0,
        })
      },

      incrementFailure: () => {
        const { consecutiveFailures, lastNotificationTime } = get()
        const newCount = consecutiveFailures + 1
        const now = Date.now()

        if (newCount >= FAILURE_THRESHOLD) {
          const shouldNotify = !lastNotificationTime || now - lastNotificationTime > NOTIFICATION_DEBOUNCE_MS

          if (shouldNotify) {
            notifications.show({
              title: 'Session expired',
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

export const getToken = (): string | null => useAuthStore.getState().token
