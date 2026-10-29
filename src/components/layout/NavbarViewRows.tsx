import type { PipelineView } from '../../types'
import { buildViewNodes, type NavNode } from '../../utils/navTree'
import { rootViewOf } from '../../utils/viewTree'

export const PINNED_ID = 'pinned'

const PINNED_FALLBACK: NavNode = { key: `view:${PINNED_ID}`, name: 'Pinned', children: [] }

export const viewTo = (id: string): string => `/pipelines?view=${encodeURIComponent(id)}`

export const viewNodesOf = (views: PipelineView[] | undefined): NavNode[] =>
  views ? buildViewNodes(views.filter((v) => v.kind !== 'all')) : [PINNED_FALLBACK]

export const activeRootOf = (views: PipelineView[] | undefined, viewId: string | null): string | null =>
  (views && viewId && rootViewOf(views, viewId)?.id) || viewId

export interface NavbarViewRow {
  key: string
  node: NavNode
  depth: number
}

export const flattenNavbarViewRows = (
  nodes: NavNode[],
  depth: number,
  isOpen: (key: string) => boolean
): NavbarViewRow[] =>
  nodes.flatMap((node) => [
    { key: node.key, node, depth },
    ...(isOpen(node.key) ? flattenNavbarViewRows(node.children, depth + 1, isOpen) : []),
  ])
