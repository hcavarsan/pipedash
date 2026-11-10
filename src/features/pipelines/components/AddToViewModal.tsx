import { Button, Checkbox, Group, Loader, Stack } from '@mantine/core'
import { IconFolder, IconPlus } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { StandardModal } from '../../../components/common/StandardModal'
import type { ViewSelectionEntry } from '../../../types/views'
import { useSetViewMembership, useViewSelection } from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { QueryBoundary } from './QueryBoundary'

interface AddToViewSessionProps {
  ids: string[]
  opened: boolean
}

const selectionNote = (rule: boolean, subView: boolean): string | undefined => {
  if (rule) {
    return 'Included by rules'
  }

  return subView ? 'Included by sub-views' : undefined
}

function ViewChoice({
  entry,
  total,
  disabled,
  pending,
  onToggle,
}: {
  entry: ViewSelectionEntry
  total: number
  disabled: boolean
  pending: boolean
  onToggle: (member: boolean) => void
}): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap" ml={(entry.depth - 1) * 24}>
      <Checkbox
        label={entry.view.name}
        description={selectionNote(entry.included_by_rule, entry.included_by_sub_view)}
        checked={entry.selected_count > 0 && entry.selected_count === total}
        indeterminate={entry.selected_count > 0 && entry.selected_count < total}
        disabled={disabled}
        onChange={() => onToggle(entry.selected_count < total)}
      />
      {pending && <Loader size="xs" role="status" aria-label={`Updating ${entry.view.name}`} />}
    </Group>
  )
}

interface ViewChoicesProps {
  entries: ViewSelectionEntry[]
  ids: string[]
  onNewView: () => void
}

function ViewChoices({ entries, ids, onNewView }: ViewChoicesProps): ReactNode {
  const membership = useSetViewMembership()

  if (entries.length === 0) {
    return (
      <EmptyState
        icon={<IconFolder size={24} aria-hidden />}
        title="No views yet"
        description="Create a view to organize your pipelines your way."
        action={
          <Button size="sm" leftSection={<IconPlus size={16} aria-hidden />} onClick={onNewView}>
            New view
          </Button>
        }
      />
    )
  }

  return (
    <Stack gap="sm">
      {entries.map((entry) => (
        <ViewChoice
          key={entry.view.id}
          entry={entry}
          total={ids.length}
          disabled={membership.isPending}
          pending={membership.isPending && membership.variables?.id === entry.view.id}
          onToggle={(member) => membership.mutate({ id: entry.view.id, pipelineIds: ids, member })}
        />
      ))}
    </Stack>
  )
}

function AddToViewSession({ ids, opened }: AddToViewSessionProps): ReactNode {
  const closeAddToView = usePipelineViewStore((s) => s.closeAddToView)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const selection = useViewSelection(ids, opened && ids.length > 0)

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
      title={ids.length === 1 ? 'Add pipeline to views' : `Add ${ids.length} pipelines to views`}
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
      <QueryBoundary query={selection} loading="Loading views…" failure="Couldn't load views">
        {(entries) => <ViewChoices entries={entries} ids={ids} onNewView={newView} />}
      </QueryBoundary>
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
