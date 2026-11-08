import type { StorageBackendType } from '../types/storage'

export const STORAGE_BACKEND_LABELS: Record<StorageBackendType, string> = {
  sqlite: 'SQLite',
  postgres: 'PostgreSQL',
}
