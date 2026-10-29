import type { StorageBackendType } from '../types/domain'

export const STORAGE_BACKEND_LABELS: Record<StorageBackendType, string> = {
  sqlite: 'SQLite',
  postgres: 'PostgreSQL',
}
