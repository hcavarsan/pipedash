import { createContext, ReactNode, useContext, useEffect, useState } from 'react'

import { tauriService } from '../services/tauri'
import type { PluginMetadata } from '../types'

interface PluginContextType {
  plugins: PluginMetadata[];
  loading: boolean;
  error: string | null;
  getPluginByType: (providerType: string) => PluginMetadata | undefined;
  getPluginDisplayName: (providerType: string) => string;
  refetch: () => Promise<void>;
}

const PluginContext = createContext<PluginContextType | undefined>(undefined)

export const PluginProvider = ({ children }: { children: ReactNode }) => {
  const [plugins, setPlugins] = useState<PluginMetadata[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const fetchPlugins = async () => {
    try {
      setLoading(true)
      setError(null)
      const availablePlugins = await tauriService.getAvailablePlugins()


      setPlugins(availablePlugins)
    } catch (err: any) {
      console.error('Failed to fetch plugins:', err)
      setError(err?.message || 'Failed to fetch available plugins')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchPlugins()
  }, [])

  const getPluginByType = (providerType: string): PluginMetadata | undefined => {
    return plugins.find((p) => p.provider_type === providerType)
  }

  const getPluginDisplayName = (providerType: string): string => {
    const plugin = getPluginByType(providerType)


    
return plugin?.name || providerType
  }

  return (
    <PluginContext.Provider
      value={{
        plugins,
        loading,
        error,
        getPluginByType,
        getPluginDisplayName,
        refetch: fetchPlugins,
      }}
    >
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
