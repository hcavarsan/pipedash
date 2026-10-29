import type { UseQueryResult } from '@tanstack/react-query'
import { type ReactNode, useState } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { type ConfigSnapshot, type MetricsConfigModalProps, metricsConfigTitle } from '../lib/metricsConfigScope'
import { useGlobalMetricsConfig, usePipelineMetricsConfig } from '../queries/useMetricsQueries'
import { MetricsConfigForm } from './MetricsConfigForm'

type ConfigGateProps = MetricsConfigModalProps & {
  query: UseQueryResult<unknown, Error>
  snapshot: ConfigSnapshot | null
}

function ConfigGate({ query, snapshot, ...props }: ConfigGateProps): ReactNode {
  const mobileShell = useMobileShell()
  const [session, setSession] = useState({ opened: props.opened, count: 0 })

  if (session.opened !== props.opened) {
    setSession({ opened: props.opened, count: props.opened ? session.count + 1 : session.count })
  }

  if (query.isError || !snapshot) {
    return (
      <StandardModal
        opened={props.opened}
        onClose={props.onClose}
        size="md"
        title={metricsConfigTitle(props, mobileShell)}
      >
        {query.isError ? (
          <ErrorState
            title="Couldn't load metrics settings"
            error={query.error}
            hint="Your saved settings were not loaded, so saving is disabled."
            onRetry={() => query.refetch()}
            retrying={query.isFetching}
          />
        ) : (
          <LoadingState message="Loading metrics settings..." />
        )}
      </StandardModal>
    )
  }

  return (
    <ErrorBoundary scope="feature" name="Metrics settings" resetKeys={[session.count]}>
      <MetricsConfigForm key={session.count} {...props} initial={snapshot} />
    </ErrorBoundary>
  )
}

function PipelineMetricsConfig(props: Extract<MetricsConfigModalProps, { scope: 'pipeline' }>): ReactNode {
  const query = usePipelineMetricsConfig(props.pipelineId)
  const snapshot = query.data ? { enabled: query.data.enabled, retentionDays: query.data.retention_days } : null

  return <ConfigGate {...props} query={query} snapshot={snapshot} />
}

function GlobalMetricsConfig(props: Extract<MetricsConfigModalProps, { scope: 'global' }>): ReactNode {
  const query = useGlobalMetricsConfig()
  const snapshot = query.data ? { enabled: query.data.enabled, retentionDays: query.data.default_retention_days } : null

  return <ConfigGate {...props} query={query} snapshot={snapshot} />
}

export function MetricsConfigModal(props: MetricsConfigModalProps): ReactNode {
  return props.scope === 'pipeline' ? <PipelineMetricsConfig {...props} /> : <GlobalMetricsConfig {...props} />
}
