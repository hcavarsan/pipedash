import { Button, Group, Select, Stack, Text, TextInput } from '@mantine/core'
import { type ReactNode, useActionState, useId, useRef } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { StandardModal } from '../../../components/common/StandardModal'
import { logger } from '../../../lib/logger'
import type { PipelineView, ViewPatch } from '../../../types/domain'
import { errorMessage } from '../../../utils/errorDisplay'
import { useConfirmDeleteView } from '../hooks/useConfirmDeleteView'
import { NAME_REQUIRED, sameJson, TOP_LEVEL, type ViewEditorDraftModel } from '../hooks/useViewEditorDraft'
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
  children: ReactNode
  onClose: () => void
  onSaved: (created: PipelineView | null) => void
}

interface SaveState {
  error: string | null
  fieldErrors: Record<string, string>
  session: number
}

const settled = (session: number): SaveState => ({ error: null, fieldErrors: {}, session })

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

export function ViewEditorForm({
  editor,
  view,
  draft,
  parentOptions,
  children,
  onClose,
  onSaved,
}: ViewEditorFormProps): ReactNode {
  const formId = useId()
  const formRef = useRef<HTMLFormElement>(null)
  const { mutateAsync: createView } = useCreatePipelineView()
  const { mutateAsync: updateView } = useUpdatePipelineView()
  const confirmDelete = useConfirmDeleteView()
  const { creating, sessionKey } = draft
  const fields = draft.draft

  const savePipelineView = async (_previous: SaveState, formData: FormData): Promise<SaveState> => {
    const entered = formData.get('name')
    const name = (typeof entered === 'string' ? entered : fields.name).trim()

    if (draft.showName && !name) {
      return { error: null, fieldErrors: { name: NAME_REQUIRED }, session: sessionKey }
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

        onSaved(created)

        return settled(sessionKey)
      }

      if (!view) {
        return settled(sessionKey)
      }

      const patch = buildViewPatch(view, name, draft)

      if (Object.keys(patch).length > 0) {
        await updateView({ id: view.id, patch })
      }
      onSaved(null)

      return settled(sessionKey)
    } catch (err) {
      logger.error('ViewEditorForm', 'Failed to save view', err)

      return { error: errorMessage(err, 'Try saving the view again.'), fieldErrors: {}, session: sessionKey }
    }
  }

  const [state, formAction, isPending] = useActionState(savePipelineView, settled(sessionKey))
  const result = state.session === sessionKey ? state : settled(sessionKey)
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

  return (
    <StandardModal
      opened={editor.open}
      onClose={onClose}
      size="lg"
      dirty={draft.dirty}
      title={creating ? (editor.parent ? 'New sub-view' : 'New view') : `Edit ${view?.name ?? 'view'}`}
      footer={footer}
      primaryAction={{
        label: creating ? 'Create' : 'Save',
        onClick: () => formRef.current?.requestSubmit(),
        loading: isPending,
      }}
    >
      <form id={formId} ref={formRef} action={formAction}>
        <Stack gap="lg">
          <FormErrorDisplay title="Couldn't save view" globalError={result.error} />

          {draft.showName && (
            <TextInput
              name="name"
              label="Name"
              required
              maxLength={60}
              value={fields.name}
              error={nameError}
              onBlur={draft.touchName}
              onChange={(event) => {
                const name = event.currentTarget.value

                draft.setDraft((d) => ({ ...d, name }))
              }}
              data-autofocus
            />
          )}

          {draft.showParent && (
            <Select
              label="Inside"
              data={parentOptions}
              value={fields.parent ?? TOP_LEVEL}
              allowDeselect={false}
              onChange={(value) =>
                value && draft.setDraft((d) => ({ ...d, parent: value === TOP_LEVEL ? null : value }))
              }
            />
          )}

          {creating && editor.initialMembers.length > 0 && (
            <Text size="sm" c="dimmed">
              {editor.initialMembers.length === 1
                ? 'The selected pipeline joins this view.'
                : `The ${editor.initialMembers.length} selected pipelines join this view.`}
            </Text>
          )}

          {children}
        </Stack>
      </form>
    </StandardModal>
  )
}
