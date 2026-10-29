import { logger } from '../lib/logger'

let isTauriCache: boolean | null = null

export const isTauri = (): boolean => {
  if (isTauriCache !== null) {
    return isTauriCache
  }

  if (typeof window === 'undefined') {
    isTauriCache = false

    return isTauriCache
  }

  const hasTauriInternals = '__TAURI_INTERNALS__' in window
  const hasTauri = '__TAURI__' in window

  isTauriCache = hasTauriInternals || hasTauri

  if (import.meta.env.DEV) {
    logger.debug('isTauri', 'Detection (cached)', { hasTauriInternals, hasTauri, result: isTauriCache })
  }

  return isTauriCache
}
