import type { PipelineView, ViewPipeline } from '../types'

const MAX_VIEW_DEPTH = 3

const findView = (views: PipelineView[], id: string) => views.find((view) => view.id === id)

export const childViews = (views: PipelineView[], id: string | null) => views.filter((view) => view.parent === id)

export const flattenViewTree = (views: PipelineView[], root: string | null = null): PipelineView[] =>
  childViews(views, root).flatMap((view) => [view, ...flattenViewTree(views, view.id)])

export const viewAncestors = (views: PipelineView[], id: string) => {
  const path: PipelineView[] = []
  let parent = findView(views, id)?.parent

  while (parent && path.length < MAX_VIEW_DEPTH) {
    const view = findView(views, parent)

    if (!view) {
      break
    }

    path.unshift(view)
    parent = view.parent
  }

  return path
}

export const viewDepth = (views: PipelineView[], id: string) => viewAncestors(views, id).length + 1

export const rootViewOf = (views: PipelineView[], id: string) => viewAncestors(views, id)[0] ?? findView(views, id)

const subtreeHeight = (views: PipelineView[], id: string): number =>
  1 + Math.max(0, ...childViews(views, id).map((view) => subtreeHeight(views, view.id)))

export const canAddSubView = (views: PipelineView[], view: PipelineView) =>
  view.kind === 'custom' && viewDepth(views, view.id) < MAX_VIEW_DEPTH

export const eligibleViewParents = (views: PipelineView[], id: string | null) => {
  const height = id === null ? 1 : subtreeHeight(views, id)
  const excluded = new Set(id === null ? [] : [id, ...flattenViewTree(views, id).map((view) => view.id)])

  return views.filter(
    (view) => view.kind === 'custom' && !excluded.has(view.id) && viewDepth(views, view.id) + height <= MAX_VIEW_DEPTH
  )
}

export const reorderSlots = (order: string[], next: string[]) => {
  const moved = new Set(next)
  let index = 0

  return order.map((id) => {
    const replacement = next[index]

    if (moved.has(id) && replacement !== undefined) {
      index += 1

      return replacement
    }

    return id
  })
}

export type DirectMembers = Record<string, string[]>

export const directMembers = (pipelines: ViewPipeline[]): DirectMembers => {
  const members: DirectMembers = {}

  for (const pipeline of pipelines) {
    for (const membership of pipeline.memberships) {
      if (membership.source !== 'sub_view') {
        members[membership.view] = [...(members[membership.view] ?? []), pipeline.id]
      }
    }
  }

  return members
}

export const setDirectMembership = (
  views: PipelineView[],
  members: DirectMembers,
  id: string,
  pipelineIds: string[],
  member: boolean
): DirectMembers => {
  if (member) {
    return { ...members, [id]: [...new Set([...(members[id] ?? []), ...pipelineIds])] }
  }

  const removed = new Set(pipelineIds)
  const cleared = [id, ...flattenViewTree(views, id).map((view) => view.id)]

  return {
    ...members,
    ...Object.fromEntries(cleared.map((view) => [view, (members[view] ?? []).filter((p) => !removed.has(p))])),
  }
}

export const viewMemberIds = (views: PipelineView[], members: DirectMembers, id: string) =>
  new Set([id, ...flattenViewTree(views, id).map((view) => view.id)].flatMap((view) => members[view] ?? []))
