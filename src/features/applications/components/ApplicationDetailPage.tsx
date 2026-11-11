import { Stack, Tabs } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { PageHeader } from '../../../components/common/PageHeader'
import type { ApplicationDetail } from '../../../types/applications'
import { ApplicationMetricsTab } from './ApplicationMetricsTab'
import { ChangesTable } from './ChangesTable'
import { DetailHeaderActions, DetailNotices } from './DetailHeaderParts'
import { FlowLane } from './FlowLane'
import { OverridesPanel } from './OverridesPanel'
import { SuggestionsDrawer } from './SuggestionsDrawer'

export type DetailTab = 'flow' | 'changes' | 'metrics'

interface ApplicationDetailPageProps {
  detail: ApplicationDetail
  tab: DetailTab
  onTabChange: (tab: DetailTab) => void
  onBack: () => void
}

const isDetailTab = (value: string | null): value is DetailTab =>
  value === 'flow' || value === 'changes' || value === 'metrics'

export function ApplicationDetailPage({ detail, tab, onTabChange, onBack }: ApplicationDetailPageProps): ReactNode {
  const [suggestionsOpen, setSuggestionsOpen] = useState(false)
  const { summary, copy, flow, suggestions } = detail

  return (
    <>
      <PageHeader
        title={summary.name}
        onBack={onBack}
        backLabel={copy.back}
        actions={
          <DetailHeaderActions detail={detail} onOpenSuggestions={() => setSuggestionsOpen(true)} onDeleted={onBack} />
        }
      />
      <DetailNotices detail={detail} />
      <Tabs value={tab} onChange={(value) => isDetailTab(value) && onTabChange(value)}>
        <Tabs.List mb="sm">
          <Tabs.Tab value="flow">{copy.tab_flow}</Tabs.Tab>
          <Tabs.Tab value="changes">{copy.tab_changes}</Tabs.Tab>
          <Tabs.Tab value="metrics">{copy.tab_metrics}</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="flow">
          <Stack gap="lg">
            <FlowLane flow={flow} copy={copy} applicationId={summary.id} canEdit={summary.capabilities.edit} />
            <OverridesPanel applicationId={summary.id} overrides={detail.overrides} copy={copy} />
          </Stack>
        </Tabs.Panel>
        <Tabs.Panel value="changes">
          <ChangesTable applicationId={summary.id} copy={copy} />
        </Tabs.Panel>
        <Tabs.Panel value="metrics">
          <ApplicationMetricsTab applicationId={summary.id} copy={copy} />
        </Tabs.Panel>
      </Tabs>
      {suggestions.length > 0 && (
        <SuggestionsDrawer
          opened={suggestionsOpen}
          onClose={() => setSuggestionsOpen(false)}
          applicationId={summary.id}
        />
      )}
    </>
  )
}
