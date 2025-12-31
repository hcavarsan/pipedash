import { useSearchParams } from 'react-router-dom'

import { UnifiedPipelinesView } from '../components/table/UnifiedPipelinesView'
import type { Pipeline, ProviderSummary } from '../types'

interface PipelinesRouteProps {
  pipelines: Pipeline[];
  providers: ProviderSummary[];
  loading?: boolean;
  onViewHistory: (pipeline: Pipeline) => void;
  onTrigger: (pipeline: Pipeline) => void;
  onViewMetrics?: (pipeline: Pipeline) => void;
}

export function PipelinesRoute({
  pipelines,
  providers,
  loading,
  onViewHistory,
  onTrigger,
  onViewMetrics,
}: PipelinesRouteProps) {
  const [searchParams] = useSearchParams()
  const providerId = searchParams.get('provider')

  const selectedProviderId = providerId ? Number(providerId) : undefined

  return (
    <UnifiedPipelinesView
      pipelines={pipelines}
      providers={providers}
      selectedProviderId={selectedProviderId}
      loading={loading}
      onViewHistory={onViewHistory}
      onTrigger={onTrigger}
      onViewMetrics={onViewMetrics}
    />
  )
}
