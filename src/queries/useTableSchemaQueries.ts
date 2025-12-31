import { useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../lib/cacheConfig'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'
import type { TableDefinition } from '../types'

export function useProviderTableSchema(providerId: number) {
  return useQuery({
    queryKey: queryKeys.tableSchema.detail(providerId),
    queryFn: () => service.getProviderTableSchema(providerId),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.LONG,
    enabled: !!providerId,
  })
}

export function useTableDefinition(providerId: number, tableId: string) {
  const { data: schema, ...query } = useProviderTableSchema(providerId)

  const table = schema?.tables.find((t: TableDefinition) => t.id === tableId) ?? null

  return {
    ...query,
    data: table,
  }
}
