import { useAuthStore } from '../stores/authStore'

export function clearApiToken(): void {
  useAuthStore.getState().clearToken()
}
