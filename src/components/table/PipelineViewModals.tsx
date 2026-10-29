import {
  ActionIcon,
  Button,
  Checkbox,
  Group,
  MultiSelect,
  SegmentedControl,
  Select,
  Stack,
  TagsInput,
  Text,
  TextInput,
  Title,
  Tooltip,
  UnstyledButton,
} from '@mantine/core'
import { IconFolder, IconGripVertical, IconPlus, IconTrash, IconX } from '@tabler/icons-react'
import { type FormEvent, type ReactNode, useCallback, useId, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { logger } from '../../lib/logger'
import {
  useCreatePipelineView,
  useDeletePipelineView,
  usePipelineViews,
  useReorderPipelineViews,
  useSetViewMembership,
  useUpdatePipelineView,
  useViewMembers,
} from '../../queries/usePipelineViewQueries'
import { useProviders } from '../../queries/useProvidersQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import { STATUS_OPTIONS } from '../../theme/status'
import type {
  PipelineStatus,
  PipelineView,
  ViewDisplay,
  ViewLayout,
  ViewPatch,
  ViewPipeline,
  ViewRules,
} from '../../types'
import { openConfirm } from '../../utils/confirm'
import {
  childViews,
  type DirectMembers,
  directMembers,
  eligibleViewParents,
  flattenViewTree,
  reorderSlots,
  setDirectMembership,
  viewAncestors,
  viewDepth,
  viewMemberIds,
} from '../../utils/viewTree'

import { EmptyState } from '../common/EmptyState'
import { SortableList } from '../common/SortableList'
import { StandardModal } from '../common/StandardModal'

const EMPTY_RULES: ViewRules = { provider_ids: [], repositories: [], name_contains: null, statuses: [] }
const DEFAULT_DISPLAY: ViewDisplay = {
  sort: 'name',
  direction: 'asc',
  sections: 'repository',
  layout: 'table',
}

const sameJson = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)

export function useConfirmDeleteView(): (view: PipelineView) => void {
  const { mutateAsync: deleteView } = useDeletePipelineView()
  const { data: views = [] } = usePipelineViews()

  return useCallback(
    (view: PipelineView) => {
      openConfirm({
        title: 'Delete view',
        children: (
          <Text size="sm">
            {`Delete "${view.name}"? Pipelines stay in Pipedash.${childViews(views, view.id).length > 0 ? ' Its sub-views move up a level.' : ''} This action cannot be undone.`}
          </Text>
        ),
        labels: { confirm: 'Delete view', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await deleteView(view.id)
            const { editor, closeEditor } = usePipelineViewStore.getState()

            if (editor.viewId === view.id) {
              closeEditor()
            }
          } catch (err) {
            logger.error('PipelineViewModals', 'Failed to delete view', err)
          }
        },
      })
    },
    [deleteView, views]
  )
}

const TOP_LEVEL = 'top'

interface Draft {
  name: string
  rules: ViewRules
  display: ViewDisplay
  parent: string | null
}

const draftOf = (view: PipelineView | undefined, parent: string | null): Draft => ({
  name: view?.name ?? '',
  rules: view?.rules ?? EMPTY_RULES,
  display: view?.display ?? DEFAULT_DISPLAY,
  parent: view ? view.parent : parent,
})

function ViewEditorModal() {
  const editor = usePipelineViewStore((s) => s.editor)
  const closeEditor = usePipelineViewStore((s) => s.closeEditor)
  const navigate = useNavigate()
  const mobileShell = useMobileShell()
  const formId = useId()
  const { data: views = [] } = usePipelineViews()
  const { data: providers = [] } = useProviders()
  const { mutateAsync: createView, isPending: creatingPending } = useCreatePipelineView()
  const { mutateAsync: updateView, isPending: updatingPending } = useUpdatePipelineView()
  const confirmDelete = useConfirmDeleteView()

  const creating = editor.viewId === null
  const view = creating ? undefined : views.find((g) => g.id === editor.viewId)
  const caps = view?.capabilities
  const showName = creating || Boolean(caps?.rename)
  const showRules = !creating && Boolean(caps?.edit_rules)
  const showLayout = !mobileShell && (creating || Boolean(caps?.edit_display))
  const showMembers = !creating && Boolean(caps?.edit_members)
  const showParent = creating || view?.kind === 'custom'
  const parentOptions = [
    { value: TOP_LEVEL, label: 'Top level' },
    ...eligibleViewParents(views, editor.viewId).map((v) => ({
      value: v.id,
      label: [...viewAncestors(views, v.id), v].map((n) => n.name).join(' / '),
    })),
  ]

  const { data: serverMembers = [] } = useViewMembers(editor.viewId ?? '', editor.open && showMembers)

  const session = editor.open ? `${editor.viewId ?? 'new'}:${view ? 'ready' : 'pending'}` : null
  const [loadedSession, setLoadedSession] = useState<string | null>(null)
  const [draft, setDraft] = useState<Draft>(() => draftOf(view, editor.parent))
  const [memberIds, setMemberIds] = useState<string[] | null>(null)
  const [nameTouched, setNameTouched] = useState(false)

  if (session !== loadedSession) {
    setLoadedSession(session)

    if (session) {
      setDraft(draftOf(view, editor.parent))
      setMemberIds(null)
      setNameTouched(false)
    }
  }

  const initial = draftOf(view, editor.parent)
  const serverIds = serverMembers.map((m) => m.pipeline_id)
  const membersById = new Map(serverMembers.map((m) => [m.pipeline_id, m]))
  const members = memberIds
    ? memberIds.flatMap((id) => {
        const member = membersById.get(id)

        return member ? [member] : []
      })
    : serverMembers
  const membersChanged = memberIds !== null && !sameJson(memberIds, serverIds)
  const dirty = !sameJson(draft, initial) || membersChanged
  const nameMissing = showName && !draft.name.trim()
  const nameError = nameTouched && nameMissing ? 'Name is required' : undefined
  const saving = creatingPending || updatingPending

  const setRules = (rules: Partial<ViewRules>) => setDraft((d) => ({ ...d, rules: { ...d.rules, ...rules } }))

  const save = async () => {
    if (nameMissing) {
      setNameTouched(true)

      return
    }

    try {
      if (creating) {
        const created = await createView({
          name: draft.name.trim(),
          rules: draft.rules,
          display: draft.display,
          members: editor.initialMembers,
          parent: draft.parent,
        })

        closeEditor()
        void navigate(`/pipelines?view=${encodeURIComponent(created.id)}`)

        return
      }

      if (!view) {
        return
      }

      const patch: ViewPatch = {}

      if (showName && draft.name.trim() !== view.name) {
        patch.name = draft.name.trim()
      }
      if (showRules && !sameJson(draft.rules, initial.rules)) {
        patch.rules = draft.rules
      }
      if (showLayout && draft.display.layout !== initial.display.layout) {
        patch.display = { ...view.display, layout: draft.display.layout }
      }
      if (showParent && draft.parent !== view.parent) {
        patch.parent = draft.parent
      }
      if (showMembers && membersChanged && memberIds) {
        patch.members = memberIds
      }
      if (Object.keys(patch).length > 0) {
        await updateView({ id: view.id, patch })
      }
      closeEditor()
    } catch (err) {
      logger.error('PipelineViewModals', 'Failed to save view', err)
    }
  }

  const onSubmit = (event: FormEvent) => {
    event.preventDefault()
    void save()
  }

  const footer = (requestClose: () => void) => (
    <Group justify="space-between" gap="sm">
      {view?.capabilities.delete ? (
        <Button variant="subtle" color="red" size="sm" onClick={() => confirmDelete(view)} disabled={saving}>
          Delete view
        </Button>
      ) : (
        <span />
      )}
      <Group gap="sm">
        <Button variant="default" size="sm" onClick={requestClose} disabled={saving}>
          Cancel
        </Button>
        <Button type="submit" form={formId} size="sm" loading={saving}>
          {creating ? 'Create view' : 'Save'}
        </Button>
      </Group>
    </Group>
  )

  return (
    <StandardModal
      opened={editor.open}
      onClose={closeEditor}
      size="lg"
      dirty={dirty}
      title={creating ? (editor.parent ? 'New sub-view' : 'New view') : `Edit ${view?.name ?? 'view'}`}
      footer={footer}
      primaryAction={{ label: creating ? 'Create' : 'Save', onClick: () => void save(), loading: saving }}
    >
      <form id={formId} onSubmit={onSubmit}>
        <Stack gap="lg">
          {showName && (
            <TextInput
              label="Name"
              required
              maxLength={60}
              value={draft.name}
              error={nameError}
              onBlur={() => setNameTouched(true)}
              onChange={(event) => {
                const name = event.currentTarget.value

                setDraft((d) => ({ ...d, name }))
              }}
              data-autofocus
            />
          )}

          {showParent && (
            <Select
              label="Inside"
              data={parentOptions}
              value={draft.parent ?? TOP_LEVEL}
              allowDeselect={false}
              onChange={(value) => value && setDraft((d) => ({ ...d, parent: value === TOP_LEVEL ? null : value }))}
            />
          )}

          {creating && editor.initialMembers.length > 0 && (
            <Text size="sm" c="dimmed">
              {editor.initialMembers.length === 1
                ? 'The selected pipeline joins this view.'
                : `The ${editor.initialMembers.length} selected pipelines join this view.`}
            </Text>
          )}

          {showRules && (
            <Stack gap="sm">
              <div>
                <Title order={3} size="sm">
                  Auto-include
                </Title>
                <Text size="xs" c="dimmed">
                  Pipelines that match every filled rule join this view automatically.
                </Text>
              </div>
              <MultiSelect
                label="Providers"
                data={providers.map((p) => ({ value: String(p.id), label: p.name }))}
                value={draft.rules.provider_ids.map(String)}
                onChange={(values) => setRules({ provider_ids: values.map(Number) })}
                clearable
                searchable
              />
              <TagsInput
                label="Repositories"
                placeholder="acme/* or acme/app"
                value={draft.rules.repositories}
                onChange={(repositories) => setRules({ repositories })}
                clearable
              />
              <TextInput
                label="Name contains"
                value={draft.rules.name_contains ?? ''}
                onChange={(event) => setRules({ name_contains: event.currentTarget.value || null })}
              />
              <MultiSelect<PipelineStatus>
                label="Statuses"
                data={STATUS_OPTIONS}
                value={draft.rules.statuses}
                onChange={(values) => setRules({ statuses: values })}
                clearable
              />
            </Stack>
          )}

          {showLayout && (
            <Stack gap="3xs">
              <Text size="sm" fw={500}>
                Layout
              </Text>
              <SegmentedControl<ViewLayout>
                aria-label="Layout"
                data={[
                  { value: 'table', label: 'Table' },
                  { value: 'cards', label: 'Cards' },
                ]}
                value={draft.display.layout}
                onChange={(value) => setDraft((d) => ({ ...d, display: { ...d.display, layout: value } }))}
              />
            </Stack>
          )}

          {showMembers && (
            <Stack gap="sm">
              <Title order={3} size="sm">
                Members
              </Title>
              {members.length === 0 ? (
                <Text size="sm" c="dimmed">
                  Use Add to view on any pipeline to add it here.
                </Text>
              ) : (
                <Stack gap="4xs">
                  <SortableList
                    items={members}
                    getId={(m) => m.pipeline_id}
                    orientation="vertical"
                    handle
                    onReorder={setMemberIds}
                  >
                    {(member, drag) => {
                      const label = member.name ?? member.pipeline_id

                      return (
                        <Group
                          ref={drag.setNodeRef}
                          style={drag.style}
                          gap="xs"
                          wrap="nowrap"
                          opacity={drag.isDragging ? 0.6 : undefined}
                        >
                          <ActionIcon
                            {...drag.handleProps}
                            {...drag.listeners}
                            aria-label={`Reorder ${label}`}
                            style={{ touchAction: 'none', cursor: 'grab' }}
                          >
                            <IconGripVertical size={16} />
                          </ActionIcon>
                          <Stack gap={0} flex={1} miw={0}>
                            <Text size="sm" truncate>
                              {label}
                            </Text>
                            {!member.available && (
                              <Text size="xs" c="dimmed">
                                Unavailable
                              </Text>
                            )}
                          </Stack>
                          <Tooltip label={`Remove ${label}`}>
                            <ActionIcon
                              aria-label={`Remove ${label}`}
                              onClick={() =>
                                setMemberIds(
                                  members.map((m) => m.pipeline_id).filter((id) => id !== member.pipeline_id)
                                )
                              }
                            >
                              <IconX size={16} />
                            </ActionIcon>
                          </Tooltip>
                        </Group>
                      )
                    }}
                  </SortableList>
                </Stack>
              )}
            </Stack>
          )}
        </Stack>
      </form>
    </StandardModal>
  )
}

function AddToViewModal() {
  const pipelines = usePipelineViewStore((s) => s.addToView)
  const closeAddToView = usePipelineViewStore((s) => s.closeAddToView)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { data: views = [] } = usePipelineViews(pipelines !== null)
  const { mutate: setMembership } = useSetViewMembership()
  const customViews = flattenViewTree(views.filter((g) => g.kind === 'custom'))

  const [loadedFor, setLoadedFor] = useState<ViewPipeline[] | null>(null)
  const [members, setMembers] = useState<DirectMembers>({})
  const [shown, setShown] = useState<ViewPipeline[]>(pipelines ?? [])

  if (pipelines !== loadedFor) {
    setLoadedFor(pipelines)

    if (pipelines) {
      setShown(pipelines)
      setMembers(directMembers(pipelines))
    }
  }

  const ruleViews = new Set(shown.flatMap((p) => p.memberships.filter((m) => m.source === 'rule').map((m) => m.view)))

  const toggle = (id: string, member: boolean) => {
    if (shown.length === 0) {
      return
    }

    const pipelineIds = shown.map((p) => p.id)
    const previous = members

    setMembers(setDirectMembership(views, members, id, pipelineIds, member))
    setMembership({ id, pipelineIds, member }, { onError: () => setMembers(previous) })
  }

  const newView = () => {
    if (shown.length === 0) {
      return
    }

    closeAddToView()
    openEditor(
      null,
      shown.map((p) => p.id)
    )
  }

  return (
    <StandardModal
      opened={pipelines !== null}
      onClose={closeAddToView}
      size="md"
      title={`Add ${shown.length === 1 ? (shown[0]?.name ?? '') : `${shown.length} pipelines`} to views`}
      footer={
        <Group justify="space-between" gap="sm">
          <Button variant="default" size="sm" leftSection={<IconPlus size={16} />} onClick={newView}>
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
          icon={<IconFolder size={24} />}
          title="No views yet"
          description="Create a view to organize your pipelines your way."
          action={
            <Button size="sm" leftSection={<IconPlus size={16} />} onClick={newView}>
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
                checked={count > 0 && count === shown.length}
                indeterminate={count > 0 && count < shown.length}
                onChange={() => toggle(g.id, count < shown.length)}
              />
            )
          })}
        </Stack>
      )}
    </StandardModal>
  )
}

function ManageViewList({
  views,
  parentId,
  movableIds,
}: {
  views: PipelineView[]
  parentId: string | null
  movableIds: string[]
}) {
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { mutate: reorder } = useReorderPipelineViews()
  const confirmDelete = useConfirmDeleteView()
  const siblings = views.filter((v) => v.parent === parentId && v.capabilities.reorder)

  if (siblings.length === 0) {
    return null
  }

  return (
    <Stack gap="4xs" {...(parentId === null ? {} : { pl: 'lg' })}>
      <SortableList
        items={siblings}
        getId={(g) => g.id}
        orientation="vertical"
        handle
        onReorder={(order) => reorder(reorderSlots(movableIds, order))}
      >
        {(view, drag) => (
          <Stack ref={drag.setNodeRef} style={drag.style} gap="4xs" opacity={drag.isDragging ? 0.6 : undefined}>
            <Group gap="xs" wrap="nowrap">
              <ActionIcon
                {...drag.handleProps}
                {...drag.listeners}
                aria-label={`Reorder ${view.name}`}
                style={{ touchAction: 'none', cursor: 'grab' }}
              >
                <IconGripVertical size={16} />
              </ActionIcon>
              <UnstyledButton
                className="interactive-surface"
                flex={1}
                miw={0}
                bdrs="sm"
                px="xs"
                py="2xs"
                onClick={() => openEditor(view.id)}
              >
                <Text size="sm" truncate>
                  {view.name}
                </Text>
              </UnstyledButton>
              {view.capabilities.delete && (
                <Tooltip label={`Delete ${view.name}`}>
                  <ActionIcon aria-label={`Delete ${view.name}`} onClick={() => confirmDelete(view)}>
                    <IconTrash size={16} />
                  </ActionIcon>
                </Tooltip>
              )}
            </Group>
            <ManageViewList views={views} parentId={view.id} movableIds={movableIds} />
          </Stack>
        )}
      </SortableList>
    </Stack>
  )
}

function ManageViewsModal() {
  const opened = usePipelineViewStore((s) => s.manageOpen)
  const closeManage = usePipelineViewStore((s) => s.closeManage)
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const { data: views = [] } = usePipelineViews(opened)
  const movableIds = views.filter((g) => g.capabilities.reorder).map((g) => g.id)
  const configViews = views.filter((g) => g.kind === 'config')

  return (
    <StandardModal
      opened={opened}
      onClose={closeManage}
      size="md"
      title="Manage views"
      footer={
        <Group justify="space-between" gap="sm">
          <Button variant="default" size="sm" leftSection={<IconPlus size={16} />} onClick={() => openEditor(null)}>
            New view
          </Button>
          <Button size="sm" onClick={closeManage}>
            Done
          </Button>
        </Group>
      }
    >
      <Stack gap="lg">
        <ManageViewList views={views} parentId={null} movableIds={movableIds} />

        {configViews.length > 0 && (
          <Stack gap="2xs">
            {configViews.map((view) => (
              <Stack key={view.id} gap={0} px="xs">
                <Text size="sm" truncate>
                  {view.name}
                </Text>
                <Text size="xs" c="dimmed">
                  From config.toml
                </Text>
              </Stack>
            ))}
          </Stack>
        )}
      </Stack>
    </StandardModal>
  )
}

export function PipelineViewModals(): ReactNode {
  return (
    <>
      <ManageViewsModal />
      <AddToViewModal />
      <ViewEditorModal />
    </>
  )
}
