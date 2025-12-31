import { createContext, ReactNode, useCallback, useContext, useMemo } from 'react'

import { usePlugins as usePluginsQuery } from '../queries/usePluginsQueries'
import type { PluginMetadata } from '../types'
import { getPluginByType, getPluginDisplayName } from '../utils/pluginHelpers'

interface PluginContextType {
  plugins: PluginMetadata[];
  loading: boolean;
  error: string | null;
  getPluginByType: (providerType: string) => PluginMetadata | undefined;
  getPluginDisplayName: (providerType: string) => string;
  refetch: () => void;
}

const PluginContext = createContext<PluginContextType | undefined>(undefined)

export const PluginProvider = ({ children }: { children: ReactNode }) => {
  const query = usePluginsQuery()

  const getPluginByTypeCallback = useCallback(
    (type: string) => getPluginByType(query.data, type),
    [query.data]
  )

  const getPluginDisplayNameCallback = useCallback(
    (type: string) => getPluginDisplayName(query.data, type),
    [query.data]
  )

  const refetchCallback = useCallback(() => {
    query.refetch()
  }, [query])

  const value = useMemo(
    () => ({
      plugins: query.data ?? [],
      loading: query.isLoading,
      error: query.error?.message ?? null,
      getPluginByType: getPluginByTypeCallback,
      getPluginDisplayName: getPluginDisplayNameCallback,
      refetch: refetchCallback,
    }),
    [query.data, query.isLoading, query.error, getPluginByTypeCallback, getPluginDisplayNameCallback, refetchCallback]
  )

  return (
    <PluginContext.Provider value={value}>
      {children}
    </PluginContext.Provider>
  )
}

export const usePlugins = () => {
  const context = useContext(PluginContext)

  if (!context) {
    throw new Error('usePlugins must be used within a PluginProvider')
  }

  return context
}
