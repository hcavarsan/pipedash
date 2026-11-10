import { IconSquare } from '@tabler/icons-react'
import { type ReactNode, useRef, useState } from 'react'
import type { ContextMenuItem } from '../../../components/common/ContextMenu'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import { useRefreshProvider } from '../../../queries/useRefreshQueries'
import type { Pipeline } from '../../../types/pipelines'
import type { ProviderSummary } from '../../../types/providers'
import type { CancellationTargets } from '../../../types/run_action_batch'
import { CancelActiveRunsModal, type CancelScope } from '../../runs/components/CancelActiveRunsModal'
import { useCancelRuns } from '../../runs/queries/useCancelRuns'

type CancelItem = Extract<ContextMenuItem, { label: string }>

interface CancelActiveRunsResult {
  cancelItem: (label: string, pipelines: Pipeline[]) => CancelItem | null
  dialog: ReactNode
}

export function useCancelActiveRuns(): CancelActiveRunsResult {
  const { mutateAsync: cancelRuns } = useCancelRuns()
  const { mutate: refreshProvider } = useRefreshProvider()
  const [scope, setScope] = useState<CancelScope | null>(null)
  const [opened, setOpened] = useState(false)
  const submitted = useRef<CancellationTargets | null>(null)

  const cancelGroups = async (targets: CancellationTargets) => {
    submitted.current ??= targets
    const { groups, batches } = submitted.current
    await cancelRuns(batches)

    for (const providerId of new Set(groups.map(({ pipeline }) => pipeline.provider_id))) {
      refreshProvider(providerId)
    }
  }

  const cancelItem = (label: string, pipelines: Pipeline[]): CancelItem | null =>
    pipelines.some((pipeline) => pipeline.actions.cancel.allowed)
      ? {
          label: 'Cancel running…',
          icon: <IconSquare size={16} aria-hidden />,
          color: 'red',
          onClick: () => {
            submitted.current = null
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
