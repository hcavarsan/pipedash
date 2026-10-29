import type { PinKind, PinnedItem, PipelineView } from '../types'
import { childViews } from './viewTree'

export interface NavNode {
  key: string
  name: string
  view?: PipelineView
  children: NavNode[]
}

export const buildViewNodes = (views: PipelineView[], parent: string | null = null): NavNode[] =>
  childViews(views, parent).map((view) => ({
    key: `view:${view.id}`,
    name: view.name,
    view,
    children: buildViewNodes(views, view.id),
  }))

export const matchesQuery = (name: string, query: string): boolean =>
  name.toLowerCase().includes(query.trim().toLowerCase())

export const filterNavTree = (nodes: NavNode[], query: string): NavNode[] =>
  query.trim() === ''
    ? nodes
    : nodes.flatMap((node) => {
        const children = filterNavTree(node.children, query)

        return children.length > 0 || matchesQuery(node.name, query) ? [{ ...node, children }] : []
      })

export const ancestorKeys = (nodes: NavNode[], key: string): string[] | null => {
  for (const node of nodes) {
    if (node.key === key) {
      return []
    }

    const found = ancestorKeys(node.children, key)

    if (found) {
      return [node.key, ...found]
    }
  }

  return null
}

export const pinTo = ({ kind, ref }: PinnedItem): string =>
  kind === 'view' ? `/pipelines?view=${encodeURIComponent(ref)}` : `/pipelines?search=${encodeURIComponent(ref)}`

export const isPinned = (pins: PinnedItem[], kind: PinKind, ref: string): boolean =>
  pins.some((pin) => pin.kind === kind && pin.ref === ref)
