import { type Dispatch, type SetStateAction, useState } from 'react'
import type { PipelineView, ViewDisplay, ViewMember, ViewRules } from '../../../types/domain'
import { useViewMembers } from '../queries/usePipelineViewQueries'
import type { ViewEditorState } from '../stores/pipelineViewStore'

export const TOP_LEVEL = 'top'
export const NAME_REQUIRED = 'Name is required'

const EMPTY_RULES: ViewRules = { provider_ids: [], repositories: [], name_contains: null, statuses: [] }
const DEFAULT_DISPLAY: ViewDisplay = {
  sort: 'name',
  direction: 'asc',
  sections: 'repository',
  layout: 'table',
}

export const sameJson = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b)

export interface ViewDraft {
  name: string
  rules: ViewRules
  display: ViewDisplay
  parent: string | null
}

const draftOf = (view: PipelineView | undefined, parent: string | null): ViewDraft => ({
  name: view?.name ?? '',
  rules: view?.rules ?? EMPTY_RULES,
  display: view?.display ?? DEFAULT_DISPLAY,
  parent: view ? view.parent : parent,
})

interface UseViewEditorDraftInput {
  editor: ViewEditorState
  view: PipelineView | undefined
  mobileShell: boolean
}

interface LoadedSession {
  session: string | null
  key: number
}

export interface ViewEditorDraftModel {
  creating: boolean
  showName: boolean
  showRules: boolean
  showLayout: boolean
  showMembers: boolean
  showParent: boolean
  draft: ViewDraft
  initial: ViewDraft
  setDraft: Dispatch<SetStateAction<ViewDraft>>
  setRules: (rules: Partial<ViewRules>) => void
  memberIds: string[] | null
  setMemberIds: Dispatch<SetStateAction<string[] | null>>
  members: ViewMember[]
  membersChanged: boolean
  dirty: boolean
  nameMissing: boolean
  nameError: string | undefined
  touchName: () => void
  sessionKey: number
}

export function useViewEditorDraft({ editor, view, mobileShell }: UseViewEditorDraftInput): ViewEditorDraftModel {
  const creating = editor.viewId === null
  const caps = view?.capabilities
  const showName = creating || Boolean(caps?.rename)
  const showRules = !creating && Boolean(caps?.edit_rules)
  const showLayout = !mobileShell && (creating || Boolean(caps?.edit_display))
  const showMembers = !creating && Boolean(caps?.edit_members)
  const showParent = creating || view?.kind === 'custom'

  const { data: serverMembers = [] } = useViewMembers(editor.viewId ?? '', editor.open && showMembers)

  const session = editor.open ? `${editor.viewId ?? 'new'}:${view ? 'ready' : 'pending'}` : null
  const [loaded, setLoaded] = useState<LoadedSession>({ session: null, key: 0 })
  const [draft, setDraft] = useState<ViewDraft>(() => draftOf(view, editor.parent))
  const [memberIds, setMemberIds] = useState<string[] | null>(null)
  const [nameTouched, setNameTouched] = useState(false)

  if (session !== loaded.session) {
    setLoaded({ session, key: session ? loaded.key + 1 : loaded.key })

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
  const nameMissing = showName && !draft.name.trim()

  return {
    creating,
    showName,
    showRules,
    showLayout,
    showMembers,
    showParent,
    draft,
    initial,
    setDraft,
    setRules: (rules) => setDraft((d) => ({ ...d, rules: { ...d.rules, ...rules } })),
    memberIds,
    setMemberIds,
    members,
    membersChanged,
    dirty: !sameJson(draft, initial) || membersChanged,
    nameMissing,
    nameError: nameTouched && nameMissing ? NAME_REQUIRED : undefined,
    touchName: () => setNameTouched(true),
    sessionKey: loaded.key,
  }
}
