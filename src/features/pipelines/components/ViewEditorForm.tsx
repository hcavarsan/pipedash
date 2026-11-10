import { Button, Group, Select, Stack, Text, TextInput } from '@mantine/core'
import type { UseQueryResult } from '@tanstack/react-query'
import { type ChangeEvent, type ReactNode, useActionState, useId, useRef } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { StandardModal } from '../../../components/common/StandardModal'
import { logger } from '../../../lib/logger'
import type { PipelineView, ViewEditorOptions, ViewPatch } from '../../../types/views'
import { errorMessage } from '../../../utils/errorDisplay'
import { sameJson } from '../../../utils/sameJson'
import { useConfirmDeleteView } from '../hooks/useConfirmDeleteView'
import { NAME_REQUIRED, TOP_LEVEL, type ViewEditorDraftModel } from '../hooks/useViewEditorDraft'
import { useCreatePipelineView, useUpdatePipelineView } from '../queries/usePipelineViewQueries'
import type { ViewEditorState } from '../stores/pipelineViewStore'

interface ParentOption {
  value: string
  label: string
}

interface ViewEditorFormProps {
  editor: ViewEditorState
  view: PipelineView | undefined
  draft: ViewEditorDraftModel
  parentOptions: ParentOption[]
  parentQuery: UseQueryResult<ViewEditorOptions, Error>
  children: (pending: boolean) => ReactNode
  onClose: () => void
  onSaved: (created: PipelineView | null) => void
}

interface SaveState {
  error: string | null
  fieldErrors: Record<string, string>
  session: number
  submitted: unknown
}

const settled = (session: number): SaveState => ({ error: null, fieldErrors: {}, session, submitted: null })

function buildViewPatch(view: PipelineView, name: string, model: ViewEditorDraftModel): ViewPatch {
  const { draft, initial, memberIds, membersChanged } = model
  const patch: ViewPatch = {}

  if (model.showName && name !== view.name) {
    patch.name = name
  }
  if (model.showRules && !sameJson(draft.rules, initial.rules)) {
    patch.rules = draft.rules
  }
  if (model.showLayout && draft.display.layout !== initial.display.layout) {
    patch.display = { ...view.display, layout: draft.display.layout }
  }
  if (model.showParent && draft.parent !== view.parent) {
    patch.parent = draft.parent
  }
  if (model.showMembers && membersChanged && memberIds) {
    patch.members = memberIds
  }

  return patch
}

interface ParentSelectProps {
  options: ParentOption[]
  value: string
  onChange: (value: string | null) => void
}

const ParentSelect = ({ options, value, onChange }: ParentSelectProps): ReactNode => (
  <Select label="Inside" data={options} value={value} allowDeselect={false} onChange={onChange} />
)

export function ViewEditorForm({
  editor,
  view,
  draft,
  parentOptions,
  parentQuery,
  children,
  onClose,
  onSaved,
}: ViewEditorFormProps): ReactNode {
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const { mutateAsync: createView } = useCreatePipelineView({ toastErrors: false })
  const { mutateAsync: updateView } = useUpdatePipelineView({ toastErrors: false })
  const confirmDelete = useConfirmDeleteView()
  const { creating, sessionKey } = draft
  const fields = draft.draft
  const draftNow = { fields, memberIds: draft.memberIds }
  const latestDraft = useRef(draftNow)
  latestDraft.current = draftNow
  const finish = (created: PipelineView | null): void => {
    if (sameJson(draftNow, latestDraft.current)) {
      onSaved(created)
    }
  }

  const savePipelineView = async (_previous: SaveState, formData: FormData): Promise<SaveState> => {
    const entered = formData.get('name')
    const name = (typeof entered === 'string' ? entered : fields.name).trim()

    if (draft.showName && !name) {
      return { ...settled(sessionKey), fieldErrors: { name: NAME_REQUIRED } }
    }

    try {
      if (creating) {
        const created = await createView({
          name,
          rules: fields.rules,
          display: fields.display,
          members: editor.initialMembers,
          parent: fields.parent,
        })

        finish(created)

        return settled(sessionKey)
      }

      if (!view) {
        return settled(sessionKey)
      }

      const patch = buildViewPatch(view, name, draft)

      if (Object.keys(patch).length > 0) {
        await updateView({ id: view.id, patch })
      }
      finish(null)

      return settled(sessionKey)
    } catch (err) {
      logger.error('ViewEditorForm', 'Failed to save view', err)

      return { ...settled(sessionKey), error: errorMessage(err, 'Try saving the view again.'), submitted: draftNow }
    }
  }

  const [state, formAction, isPending] = useActionState(savePipelineView, settled(sessionKey))
  const result = state.session === sessionKey ? state : settled(sessionKey)
  const saveError = sameJson(result.submitted, draftNow) ? result.error : null
  const nameError = draft.nameError ?? (draft.nameMissing ? result.fieldErrors.name : undefined)

  const footer = (requestClose: () => void): ReactNode => (
    <Group justify="space-between" gap="sm">
      {view?.capabilities.delete ? (
        <Button variant="subtle" color="red" size="sm" onClick={() => confirmDelete(view)} disabled={isPending}>
          Delete view
        </Button>
      ) : (
        <span />
      )}
      <Group gap="sm">
        <Button variant="default" size="sm" onClick={requestClose} disabled={isPending}>
          Cancel
        </Button>
        <Button type="submit" form={formId} size="sm" loading={isPending}>
          {creating ? 'Create view' : 'Save'}
        </Button>
      </Group>
    </Group>
  )

  let title = `Edit ${view?.name ?? 'view'}`

  if (creating) {
    title = editor.parent ? 'New sub-view' : 'New view'
  }

  const memberCount = editor.initialMembers.length
  const memberNote =
    memberCount === 1
      ? 'The selected pipeline joins this view.'
      : `The ${memberCount} selected pipelines join this view.`
  const onNameChange = (event: ChangeEvent<HTMLInputElement>): void => {
    const name = event.currentTarget.value

    draft.setDraft((d) => ({ ...d, name }))
  }
  const onParentChange = (value: string | null): void => {
    if (value) {
      draft.setDraft((d) => ({ ...d, parent: value === TOP_LEVEL ? null : value }))
    }
  }
  const parentSelect = (
    <ParentSelect options={parentOptions} value={draft.draft.parent ?? TOP_LEVEL} onChange={onParentChange} />
  )
  const fieldsBody = (
    <Stack gap="lg">
      <FormErrorDisplay title="Couldn't save view" globalError={saveError} />

      {draft.showName && (
        <TextInput
          name="name"
          label="Name"
          required
          maxLength={60}
          value={fields.name}
          error={nameError}
          onBlur={draft.touchName}
          onChange={onNameChange}
          data-autofocus
        />
      )}

      {draft.showParent && (
        <QueryBoundary
          query={parentQuery}
          loading="Loading parent views…"
          failure="Couldn't load parent views"
          variant="compact"
        >
          {() => parentSelect}
        </QueryBoundary>
      )}

      {creating && editor.initialMembers.length > 0 && (
        <Text size="sm" c="dimmed">
          {memberNote}
        </Text>
      )}

      {children(isPending)}
    </Stack>
  )

  return (
    <StandardModal
      opened={editor.open}
      onClose={onClose}
      size="lg"
      dirty={draft.dirty}
      loading={isPending}
      title={title}
      footer={footer}
      primaryAction={{
        label: creating ? 'Create view' : 'Save',
        onClick: () => formRef.current?.requestSubmit(),
        loading: isPending,
      }}
    >
      <form id={formId} ref={formRef} action={formAction}>
        <fieldset
          disabled={isPending}
          style={{ border: 0, padding: 0, margin: 0, minInlineSize: 0, display: 'contents' }}
        >
          {fieldsBody}
        </fieldset>
      </form>
    </StandardModal>
  )
}
