import { notifications } from '@mantine/notifications'
import { IconSquare } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useState } from 'react'

import type { ContextMenuItem } from '../components/common/ContextMenu'
import { CancelActiveRunsModal, type CancelScope } from '../components/pipeline/CancelActiveRunsModal'
import { queryKeys } from '../lib/queryKeys'
import { usePipelines } from '../queries/usePipelinesQueries'
import { usePlugins } from '../queries/usePluginsQueries'
import { useRefreshProvider } from '../queries/useRefreshQueries'
import { service } from '../services'
import type { Pipeline, ProviderSummary } from '../types'
import {
  type CancelGroup,
  type CancelResult,
  summarizeCancellation,
  unfinishedPipelines,
} from '../utils/cancelActiveRuns'
import { errorMessage } from '../utils/errorDisplay'
import { getPluginByType } from '../utils/pluginHelpers'

type CancelItem = Extract<ContextMenuItem, { label: string }>

interface CancelActiveRunsResult {
  cancelItem: (label: string, pipelines: Pipeline[]) => CancelItem | null
  dialog: ReactNode
}

export function useCancelActiveRuns(): CancelActiveRunsResult {
  const queryClient = useQueryClient()
  const { data: plugins } = usePlugins()
  const { mutate: refreshProvider } = useRefreshProvider()
  const [scope, setScope] = useState<CancelScope | null>(null)
  const [opened, setOpened] = useState(false)

  const cancelGroups = async (groups: CancelGroup[]) => {
    const results: CancelResult[] = []

    for (const { pipeline, runs } of groups) {
      for (const { run_number: runNumber } of runs) {
        const error = await service.cancelPipelineRun(pipeline.id, runNumber).then(
          () => null,
          (reason) => errorMessage(reason, 'Cancellation failed')
        )

        results.push({ pipelineId: pipeline.id, pipelineName: pipeline.name, runNumber, error })
      }
    }

    for (const { pipelineId, runNumber, error } of results) {
      if (error === null) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.runs.detail(pipelineId, runNumber) })
      }
    }

    for (const { pipeline } of groups) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipeline.id) })
    }

    for (const providerId of new Set(groups.map(({ pipeline }) => pipeline.provider_id))) {
      refreshProvider(providerId)
    }

    notifications.show(summarizeCancellation(results))
  }

  const cancelItem = (label: string, pipelines: Pipeline[]): CancelItem | null =>
    unfinishedPipelines(pipelines).length === 0 &&
    !pipelines.some((pipeline) => getPluginByType(plugins, pipeline.provider_type)?.capabilities.cancel)
      ? null
      : {
          label: 'Cancel running…',
          icon: <IconSquare size={16} aria-hidden />,
          color: 'red',
          onClick: () => {
            setScope({ label, pipelines })
            setOpened(true)
          },
        }

  const dialog = (
    <CancelActiveRunsModal opened={opened} scope={scope} onClose={() => setOpened(false)} onConfirm={cancelGroups} />
  )

  return { cancelItem, dialog }
}

interface ProviderCancelItemsResult {
  providerItems: (provider: ProviderSummary) => CancelItem[]
  dialog: ReactNode
}

export function useProviderCancelItems(providers: ProviderSummary[]): ProviderCancelItemsResult {
  const { data: pipelines = [] } = usePipelines(undefined, { enabled: providers.length > 0 })
  const { cancelItem, dialog } = useCancelActiveRuns()

  const providerItems = (provider: ProviderSummary): CancelItem[] => {
    const item = cancelItem(
      provider.name,
      pipelines.filter((pipeline) => pipeline.provider_id === provider.id)
    )

    return item ? [item] : []
  }

  return { providerItems, dialog }
}
