import { IconSquare } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useState } from 'react'
import type { ContextMenuItem } from '../../../components/common/ContextMenu'
import { notifications } from '../../../lib/notifications'
import { queryKeys } from '../../../lib/queryKeys'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import { useRefreshProvider } from '../../../queries/useRefreshQueries'
import { service } from '../../../services/service'
import type { Pipeline } from '../../../types/pipelines'
import type { ProviderSummary } from '../../../types/providers'
import type { CancelGroup } from '../../../types/run_action_batch'
import { runTargetsOf, summarizeCancellation } from '../../../utils/cancelActiveRuns'
import { CancelActiveRunsModal, type CancelScope } from '../../runs/components/CancelActiveRunsModal'

type CancelItem = Extract<ContextMenuItem, { label: string }>

interface CancelActiveRunsResult {
  cancelItem: (label: string, pipelines: Pipeline[]) => CancelItem | null
  dialog: ReactNode
}

export function useCancelActiveRuns(): CancelActiveRunsResult {
  const queryClient = useQueryClient()
  const { mutate: refreshProvider } = useRefreshProvider()
  const [scope, setScope] = useState<CancelScope | null>(null)
  const [opened, setOpened] = useState(false)

  const cancelGroups = async (groups: CancelGroup[]) => {
    const batch = await service.cancelPipelineRuns(runTargetsOf(groups))

    for (const { target, error } of batch.results) {
      if (error === null) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.runs.detail(target.pipeline_id, target.run_number) })
      }
    }

    for (const { pipeline } of groups) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipeline.id) })
    }

    for (const providerId of new Set(groups.map(({ pipeline }) => pipeline.provider_id))) {
      refreshProvider(providerId)
    }

    notifications.show(summarizeCancellation(batch))
  }

  const cancelItem = (label: string, pipelines: Pipeline[]): CancelItem | null =>
    pipelines.some((pipeline) => pipeline.actions.cancel.allowed)
      ? {
          label: 'Cancel running…',
          icon: <IconSquare size={16} aria-hidden />,
          color: 'red',
          onClick: () => {
            setScope({ label, pipelines })
            setOpened(true)
          },
        }
      : null

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
