import { Button, Checkbox, Group, Stack } from '@mantine/core'
import { IconFolder, IconPlus } from '@tabler/icons-react'
import { notifyManager, useQueryClient } from '@tanstack/react-query'
import { type ReactNode, startTransition, useOptimistic, useState, useSyncExternalStore } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { StandardModal } from '../../../components/common/StandardModal'
import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import type { PipelineViewPage, ViewPipeline } from '../../../types/domain'
import {
  type DirectMembers,
  directMembers,
  flattenViewTree,
  setDirectMembership,
  viewDepth,
  viewMemberIds,
} from '../../../utils/viewTree'
import { usePipelineViews, useSetViewMembership } from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'

interface MembershipChange {
  id: string
  pipelineIds: string[]
  member: boolean
}

interface AddToViewSessionProps {
  ids: string[]
  opened: boolean
}

function useViewPipelines(ids: string[]): ViewPipeline[] {
  const queryClient = useQueryClient()
  const cache = queryClient.getQueryCache()

  useSyncExternalStore(
    (notify) => cache.subscribe(notifyManager.batchCalls(notify)),
    () =>
      cache
        .findAll({ queryKey: queryKeys.pipelines.pages() })
        .map((query) => `${query.queryHash}:${query.state.dataUpdatedAt}`)
        .join('|')
  )

  const wanted = new Set(ids)
  const found = new Map<string, ViewPipeline>()

  for (const [, page] of queryClient.getQueriesData<PipelineViewPage>({ queryKey: queryKeys.pipelines.pages() })) {
    if (!page) {
      continue
    }

    const all = [
      ...page.pinned,
      ...page.pinned_blocks.flatMap((b) => b.pipelines),
      ...page.sections.flatMap((s) => s.pipelines),
    ]

    for (const pipeline of all) {
      if (wanted.has(pipeline.id)) {
        found.set(pipeline.id, pipeline)
      }
    }
  }

  return ids.flatMap((id) => {
    const pipeline = found.get(id)

    return pipeline ? [pipeline] : []
  })
}

function AddToViewSession({ ids, opened }: AddToViewSessionProps): ReactNode {
  const queryClient = useQueryClient()
  const closeAddToView = usePipelineViewStore((s) => s.closeAddToView)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { data: views = [] } = usePipelineViews(opened)
  const { mutateAsync: setMembership } = useSetViewMembership()
  const pipelines = useViewPipelines(ids)
  const [members, setOptimisticMembers] = useOptimistic<DirectMembers, MembershipChange>(
    directMembers(pipelines),
    (current, change) => setDirectMembership(views, current, change.id, change.pipelineIds, change.member)
  )
  const customViews = flattenViewTree(views.filter((g) => g.kind === 'custom'))
  const ruleViews = new Set(
    pipelines.flatMap((p) => p.memberships.filter((m) => m.source === 'rule').map((m) => m.view))
  )

  const toggle = (id: string, member: boolean) => {
    if (ids.length === 0) {
      return
    }

    startTransition(async () => {
      setOptimisticMembers({ id, pipelineIds: ids, member })

      try {
        await setMembership({ id, pipelineIds: ids, member })
        await queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
      } catch (err) {
        logger.error('AddToViewModal', 'Failed to update view membership', err)
      }
    })
  }

  const newView = () => {
    if (ids.length === 0) {
      return
    }

    closeAddToView()
    openEditor(null, ids)
  }

  return (
    <StandardModal
      opened={opened}
      onClose={closeAddToView}
      size="md"
      title={`Add ${ids.length === 1 ? (pipelines[0]?.name ?? '') : `${ids.length} pipelines`} to views`}
      footer={
        <Group justify="space-between" gap="sm">
          <Button variant="default" size="sm" leftSection={<IconPlus size={16} aria-hidden />} onClick={newView}>
            New view…
          </Button>
          <Button size="sm" onClick={closeAddToView}>
            Done
          </Button>
        </Group>
      }
    >
      {customViews.length === 0 ? (
        <EmptyState
          icon={<IconFolder size={24} aria-hidden />}
          title="No views yet"
          description="Create a view to organize your pipelines your way."
          action={
            <Button size="sm" leftSection={<IconPlus size={16} aria-hidden />} onClick={newView}>
              New view
            </Button>
          }
        />
      ) : (
        <Stack gap="sm">
          {customViews.map((g) => {
            const count = viewMemberIds(views, members, g.id).size
            const own = members[g.id]?.length ?? 0

            return (
              <Checkbox
                key={g.id}
                ml={(viewDepth(views, g.id) - 1) * 24}
                label={g.name}
                description={
                  own > 0 && ruleViews.has(g.id)
                    ? 'Included by rules'
                    : count > own
                      ? 'Included by sub-views'
                      : undefined
                }
                checked={count > 0 && count === ids.length}
                indeterminate={count > 0 && count < ids.length}
                onChange={() => toggle(g.id, count < ids.length)}
              />
            )
          })}
        </Stack>
      )}
    </StandardModal>
  )
}

export function AddToViewModal(): ReactNode {
  const ids = usePipelineViewStore((s) => s.addToViewIds)
  const [shownIds, setShownIds] = useState<string[]>([])

  if (ids !== null && ids !== shownIds) {
    setShownIds(ids)
  }

  return <AddToViewSession ids={ids ?? shownIds} opened={ids !== null} />
}
