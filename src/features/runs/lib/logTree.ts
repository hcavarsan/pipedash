import type { RunLogNode, RunLogTree } from '../../../types/logs'

export interface LogNodeGroup {
  stage: string | null
  nodes: RunLogNode[]
  skipped: RunLogNode[]
}

export type JobListRow =
  | { kind: 'node'; node: RunLogNode }
  | { kind: 'skipped'; stageKey: string; count: number; open: boolean }

type LayoutTree = Pick<RunLogTree, 'nodes' | 'groups'>

export const stageKey = (stage: string | null): string => JSON.stringify(stage)

export const skippedLabel = (count: number): string => `${count} skipped ${count === 1 ? 'job' : 'jobs'}`

export function toggleStage(open: ReadonlySet<string>, key: string): ReadonlySet<string> {
  const next = new Set(open)

  if (!next.delete(key)) {
    next.add(key)
  }

  return next
}

export function resolveLogGroups(tree: LayoutTree, failedFirst = false): LogNodeGroup[] {
  const byId = new Map(tree.nodes.map((node) => [node.id, node]))
  const resolve = (ids: string[]) => ids.flatMap((id) => byId.get(id) ?? [])

  return tree.groups.map((group) => ({
    stage: group.stage,
    nodes: resolve(failedFirst ? group.failed_first_node_ids : group.node_ids),
    skipped: resolve(group.skipped_node_ids),
  }))
}

export function jobListRows(
  tree: LayoutTree,
  openStages: ReadonlySet<string>,
  selectedId: string | null
): JobListRow[] {
  const navigable = tree.nodes.filter((node) => node.navigable)
  const position = new Map(navigable.map((node, index) => [node.id, index]))
  const hidden = new Set<string>()
  const folds = new Map<number, JobListRow[]>()

  for (const group of tree.groups) {
    const skipped = group.skipped_node_ids

    if (skipped.length === 0) {
      continue
    }

    const key = stageKey(group.stage)
    const open = openStages.has(key) || (selectedId !== null && skipped.includes(selectedId))

    if (!open) {
      for (const id of skipped) {
        hidden.add(id)
      }
    }

    const anchor = Math.max(...[...group.node_ids, ...skipped].map((id) => position.get(id) ?? -1))
    folds.set(anchor, [...(folds.get(anchor) ?? []), { kind: 'skipped', stageKey: key, count: skipped.length, open }])
  }

  return navigable.flatMap((node, index): JobListRow[] => [
    ...(hidden.has(node.id) ? [] : [{ kind: 'node' as const, node }]),
    ...(folds.get(index) ?? []),
  ])
}
