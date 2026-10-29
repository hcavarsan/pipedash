import { Tabs } from '@mantine/core'
import { IconChartLine, IconHistory } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { useBulkCancelRuns } from '../../hooks/useBulkCancelRuns'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { usePlugins } from '../../queries/usePluginsQueries'
import type { Pipeline } from '../../types'
import { getPluginDisplayName } from '../../utils/pluginHelpers'
import { LoadingState } from '../common/LoadingState'
import { PageContainer } from '../common/PageContainer'
import { ErrorBoundary } from '../ErrorBoundary'
import { PullToRefresh } from '../layout/PullRefreshContext'
import { PipelineMetricsView } from '../pipeline/PipelineMetricsView'

import { useRunActions } from './RunActionsContext'
import { RunHistoryDialogs } from './RunHistoryDialogs'
import { RunHistoryFilters } from './RunHistoryFilters'
import { RunHistoryHeader } from './RunHistoryHeader'
import { RunHistoryResults } from './RunHistoryResults'
import { useRunHistoryColumns } from './useRunHistoryColumns'
import { useRunHistoryEvents } from './useRunHistoryEvents'
import { useRunHistoryPageData } from './useRunHistoryPageData'

export type DetailTab = 'history' | 'metrics'

interface RunHistoryPageProps {
  pipeline: Pipeline | null
  tab: DetailTab
  onTabChange: (tab: DetailTab) => void
  onBack: () => void
  isLoadingPipeline?: boolean | undefined
}

export function RunHistoryPage({
  pipeline,
  tab,
  onTabChange,
  onBack,
  isLoadingPipeline = false,
}: RunHistoryPageProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const { data: plugins } = usePlugins()
  const { labels, canCancel, cancellingRun } = useRunActions()
  const data = useRunHistoryPageData(pipeline)
  const columns = useRunHistoryColumns(pipeline?.provider_id)
  const [customization, setCustomization] = useState({ session: 0, opened: false })

  useRunHistoryEvents({
    pipelineId: pipeline?.id,
    resetPage: () => data.setFilter('page', 1),
    refetch: () => {
      void data.refetch()
    },
  })

  const selection = useBulkCancelRuns({
    pipeline: pipeline ?? undefined,
    page: isMobile ? 1 : data.filters.page,
    runs: data.runs,
    enabled: canCancel,
    cancellingRunNumber: cancellingRun,
  })

  const historyLoading = isLoadingPipeline || data.isLoading || columns.loading || !pipeline?.id

  return (
    <PageContainer>
      {mobileShell && <PullToRefresh refresh={data.refetch} />}
      <RunHistoryHeader
        pipeline={pipeline}
        isLoadingPipeline={isLoadingPipeline}
        showCustomize={tab === 'history'}
        customizationDisabled={columns.loading || !!columns.error}
        onBack={onBack}
        onCustomize={() => setCustomization((previous) => ({ session: previous.session + 1, opened: true }))}
      />

      <Tabs
        value={tab}
        onChange={(value) => onTabChange(value === 'metrics' ? 'metrics' : 'history')}
        style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}
      >
        <Tabs.List mb="sm">
          <Tabs.Tab value="history" leftSection={mobileShell ? undefined : <IconHistory size={16} aria-hidden />}>
            {labels.history}
          </Tabs.Tab>
          <Tabs.Tab value="metrics" leftSection={mobileShell ? undefined : <IconChartLine size={16} aria-hidden />}>
            Metrics
          </Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel
          value="history"
          style={{
            width: '100%',
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--mantine-spacing-md)',
            flex: 1,
            minHeight: 0,
          }}
        >
          {historyLoading || !pipeline ? (
            <LoadingState message={`Loading ${labels.history.toLowerCase()}…`} />
          ) : (
            <>
              <RunHistoryFilters pipeline={pipeline} data={data} selection={selection} />
              <RunHistoryResults data={data} columns={columns} selection={selection} />
            </>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="metrics" style={{ width: '100%' }}>
          {isLoadingPipeline || !pipeline ? (
            <LoadingState message="Loading pipeline…" />
          ) : (
            <ErrorBoundary scope="feature" name="Pipeline metrics">
              <PipelineMetricsView
                pipelineId={pipeline.id}
                pipelineName={pipeline.name}
                repository={pipeline.repository}
              />
            </ErrorBoundary>
          )}
        </Tabs.Panel>
      </Tabs>

      <RunHistoryDialogs
        pipeline={pipeline}
        columns={columns}
        customizationSession={customization.session}
        customizationOpened={customization.opened}
        onCloseCustomization={() => setCustomization((previous) => ({ ...previous, opened: false }))}
        providerName={pipeline ? getPluginDisplayName(plugins, pipeline.provider_type) : ''}
      />
    </PageContainer>
  )
}
