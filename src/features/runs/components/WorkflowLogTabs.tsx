import { Box, ScrollArea, Tabs } from '@mantine/core'
import { type ReactNode, useRef, useState } from 'react'

import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import type { LogViewerHandle } from '../../logs/components/LogViewer'
import { createLogShortcutHandler } from '../lib/logShortcuts'
import { RunLogsPanel } from './RunLogsPanel'

interface WorkflowLogTabsProps {
  pipelineId: string
  pipelineName: string
  runNumber: number
  providerName: string
  narrow: boolean
  details: ReactNode
}

const column = { display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 } as const

export function WorkflowLogTabs({
  pipelineId,
  pipelineName,
  runNumber,
  providerName,
  narrow,
  details,
}: WorkflowLogTabsProps): ReactNode {
  const [tab, setTab] = useState<'logs' | 'details'>('logs')
  const viewer = useRef<LogViewerHandle | null>(null)

  const attachViewer = (handle: LogViewerHandle | null): (() => void) | undefined => {
    viewer.current = handle
    handle?.focusBody()

    return () => {
      viewer.current = null
    }
  }

  return (
    <Tabs
      value={tab}
      onChange={(value) => setTab(value === 'details' ? 'details' : 'logs')}
      keepMounted={false}
      onKeyDown={tab === 'logs' ? createLogShortcutHandler(() => viewer.current) : undefined}
      style={column}
    >
      <Tabs.List px="lg">
        <Tabs.Tab value="logs">Logs</Tabs.Tab>
        <Tabs.Tab value="details">Details</Tabs.Tab>
      </Tabs.List>
      <Tabs.Panel value="logs" style={column}>
        <ErrorBoundary scope="feature" name="Run logs">
          <RunLogsPanel
            key={`${pipelineId}:${runNumber}`}
            pipelineId={pipelineId}
            pipelineName={pipelineName}
            runNumber={runNumber}
            providerName={providerName}
            narrow={narrow}
            viewerRef={attachViewer}
          />
        </ErrorBoundary>
      </Tabs.Panel>
      <Tabs.Panel value="details" style={column}>
        <ScrollArea style={{ flex: 1, minHeight: 0 }}>
          <Box p="lg">{details}</Box>
        </ScrollArea>
      </Tabs.Panel>
    </Tabs>
  )
}
