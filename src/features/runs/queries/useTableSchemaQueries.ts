import { type UseQueryResult, useQuery } from '@tanstack/react-query'

import { GC_TIMES, STALE_TIMES } from '../../../lib/cacheConfig'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { TableDefinition, TableSchema } from '../../../types/domain'

export function useProviderTableSchema(providerId: number): UseQueryResult<TableSchema, Error> {
  return useQuery({
    queryKey: queryKeys.tableSchema.detail(providerId),
    queryFn: () => service.getProviderTableSchema(providerId),
    staleTime: STALE_TIMES.SLOW_CHANGING,
    gcTime: GC_TIMES.LONG,
    enabled: !!providerId,
  })
}

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

type UseTableDefinitionResult = DistributiveOmit<UseQueryResult<TableSchema, Error>, 'data'> & {
  data: TableDefinition | null
}

export function useTableDefinition(providerId: number, tableId: string): UseTableDefinitionResult {
  const { data: schema, ...query } = useProviderTableSchema(providerId)

  const table = schema?.tables.find((t: TableDefinition) => t.id === tableId) ?? null

  return {
    ...query,
    data: table,
  }
}
