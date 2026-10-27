import type { RunLogNode } from '../../types/logs'

export interface LogNodeGroup {
  stage: string | null
  nodes: RunLogNode[]
}

export function groupLogNodes(nodes: readonly RunLogNode[], failedFirst = false): LogNodeGroup[] {
  const byId = new Map(nodes.map((node) => [node.id, node]))
  const groups = new Map<string | null, RunLogNode[]>()

  for (const node of nodes) {
    if (node.log === null) {
      continue
    }

    let stage: string | null = null

    for (let parent = node.parent_id ? byId.get(node.parent_id) : undefined; parent; ) {
      if (parent.kind === 'stage') {
        stage = parent.name
        break
      }

      parent = parent.parent_id ? byId.get(parent.parent_id) : undefined
    }

    groups.set(stage, [...(groups.get(stage) ?? []), node])
  }

  return Array.from(groups, ([stage, members]) => ({
    stage,
    nodes: failedFirst
      ? [...members.filter((node) => node.status === 'failed'), ...members.filter((node) => node.status !== 'failed')]
      : members,
  }))
}
