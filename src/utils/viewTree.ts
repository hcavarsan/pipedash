import type { PipelineView } from '../types/views'

const findView = (views: PipelineView[], id: string) => views.find((view) => view.id === id)

export const childViews = (views: PipelineView[], id: string | null): PipelineView[] =>
  views.filter((view) => view.parent === id)

export const viewAncestors = (views: PipelineView[], id: string): PipelineView[] => {
  const path: PipelineView[] = []
  let parent = findView(views, id)?.parent

  while (parent) {
    const view = findView(views, parent)

    if (!view) {
      break
    }

    path.unshift(view)
    parent = view.parent
  }

  return path
}

export const rootViewOf = (views: PipelineView[], id: string): PipelineView | undefined =>
  viewAncestors(views, id)[0] ?? findView(views, id)

export const reorderSlots = (order: string[], next: string[]): string[] => {
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
