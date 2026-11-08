import type { RunLogNode, RunLogTree } from '../../../types/logs'

export interface LogNodeGroup {
  stage: string | null
  nodes: RunLogNode[]
}

export function resolveLogGroups(tree: Pick<RunLogTree, 'nodes' | 'groups'>, failedFirst = false): LogNodeGroup[] {
  const byId = new Map(tree.nodes.map((node) => [node.id, node]))

  return tree.groups.map((group) => ({
    stage: group.stage,
    nodes: (failedFirst ? group.failed_first_node_ids : group.node_ids).flatMap((id) => byId.get(id) ?? []),
  }))
}
