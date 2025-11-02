import { useCallback, useState } from 'react'

import { tauriService } from '../services/tauri'
import type {
  AggregatedMetrics,
  AggregationPeriod,
  AggregationType,
  GlobalMetricsConfig,
  MetricsConfig,
  MetricsStats,
  MetricType,
} from '../types'

export const useMetrics = () => {
  const [configLoading, setConfigLoading] = useState(false)
  const [metricsLoading, setMetricsLoading] = useState(false)
  const [statsLoading, setStatsLoading] = useState(false)
  const [flushLoading, setFlushLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const getGlobalConfig = useCallback(async (): Promise<GlobalMetricsConfig | null> => {
    try {
      setConfigLoading(true)
      setError(null)

return await tauriService.getGlobalMetricsConfig()
    } catch (err: any) {
      const errorMsg = err?.error || err?.message || 'Failed to get metrics config'

      setError(errorMsg)

return null
    } finally {
      setConfigLoading(false)
    }
  }, [])

  const updateGlobalConfig = useCallback(
    async (enabled: boolean, retentionDays: number): Promise<boolean> => {
      try {
        setConfigLoading(true)
        setError(null)
        await tauriService.updateGlobalMetricsConfig(enabled, retentionDays)

return true
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to update metrics config'

        setError(errorMsg)

return false
      } finally {
        setConfigLoading(false)
      }
    },
    []
  )

  const getPipelineConfig = useCallback(
    async (pipelineId: string): Promise<MetricsConfig | null> => {
      try {
        setConfigLoading(true)
        setError(null)

return await tauriService.getPipelineMetricsConfig(pipelineId)
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to get pipeline metrics config'

        setError(errorMsg)

return null
      } finally {
        setConfigLoading(false)
      }
    },
    []
  )

  const updatePipelineConfig = useCallback(
    async (pipelineId: string, enabled: boolean, retentionDays: number): Promise<boolean> => {
      try {
        setConfigLoading(true)
        setError(null)
        await tauriService.updatePipelineMetricsConfig(pipelineId, enabled, retentionDays)

return true
      } catch (err: any) {
        const errorMsg =
          err?.error || err?.message || 'Failed to update pipeline metrics config'

        setError(errorMsg)

return false
      } finally {
        setConfigLoading(false)
      }
    },
    []
  )

  const queryAggregatedMetrics = useCallback(
    async (
      metricType: MetricType,
      aggregationPeriod: AggregationPeriod,
      aggregationType?: AggregationType,
      pipelineId?: string,
      startDate?: string,
      endDate?: string,
      limit?: number
    ): Promise<AggregatedMetrics | null> => {
      try {
        setMetricsLoading(true)
        setError(null)

return await tauriService.queryAggregatedMetrics(
          metricType,
          aggregationPeriod,
          aggregationType,
          pipelineId,
          startDate,
          endDate,
          limit
        )
      } catch (err: any) {
        const errorMsg = err?.error || err?.message || 'Failed to query metrics'

        setError(errorMsg)

return null
      } finally {
        setMetricsLoading(false)
      }
    },
    []
  )

  const getStorageStats = useCallback(async (): Promise<MetricsStats | null> => {
    try {
      setStatsLoading(true)
      setError(null)

return await tauriService.getMetricsStorageStats()
    } catch (err: any) {
      const errorMsg = err?.error || err?.message || 'Failed to get storage stats'

      setError(errorMsg)

return null
    } finally {
      setStatsLoading(false)
    }
  }, [])

  const flushMetrics = useCallback(async (pipelineId?: string): Promise<number> => {
    try {
      setFlushLoading(true)
      setError(null)
      const deleted = await tauriService.flushPipelineMetrics(pipelineId)

return deleted
    } catch (err: any) {
      const errorMsg = err?.error || err?.message || 'Failed to flush metrics'

      setError(errorMsg)

return 0
    } finally {
      setFlushLoading(false)
    }
  }, [])

  return {
    configLoading,
    metricsLoading,
    statsLoading,
    flushLoading,
    error,
    getGlobalConfig,
    updateGlobalConfig,
    getPipelineConfig,
    updatePipelineConfig,
    queryAggregatedMetrics,
    getStorageStats,
    flushMetrics,
  }
}
