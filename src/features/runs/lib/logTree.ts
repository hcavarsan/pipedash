import type { RunLogNode } from '../../../types/logs'

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
    const seen = new Set<string>([node.id])

    for (let parent = node.parent_id ? byId.get(node.parent_id) : undefined; parent && !seen.has(parent.id); ) {
      if (parent.kind === 'stage') {
        stage = parent.name
        break
      }

      seen.add(parent.id)
      parent = parent.parent_id ? byId.get(parent.parent_id) : undefined
    }

    const members = groups.get(stage)

    if (members) {
      members.push(node)
    } else {
      groups.set(stage, [node])
    }
  }

  return Array.from(groups, ([stage, members]) => ({
    stage,
    nodes: failedFirst
      ? [...members.filter((node) => node.status === 'failed'), ...members.filter((node) => node.status !== 'failed')]
      : members,
  }))
}
