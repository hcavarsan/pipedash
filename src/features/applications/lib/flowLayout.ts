import type { Flow, FlowNode } from '../../../types/applications'

export const NODE_WIDTH = 180
export const NODE_HEIGHT = 72
export const NODE_GAP = 40
export const MIN_SCALE = 0.6

const STEP_X = NODE_WIDTH + NODE_GAP
const STEP_Y = NODE_HEIGHT + NODE_GAP

export interface Point {
  x: number
  y: number
}

export interface LinkGeometry {
  path: string
  mid: Point
}

export function nodePosition(node: Pick<FlowNode, 'column' | 'row'>, vertical: boolean): Point {
  if (vertical) {
    return { x: node.row * STEP_X, y: node.column * STEP_Y }
  }

  return { x: node.column * STEP_X, y: node.row * STEP_Y }
}

export function flowSize(flow: Pick<Flow, 'columns' | 'rows'>, vertical: boolean): { width: number; height: number } {
  const across = vertical ? flow.rows : flow.columns
  const down = vertical ? flow.columns : flow.rows

  return {
    width: Math.max(0, across * STEP_X - NODE_GAP),
    height: Math.max(0, down * STEP_Y - NODE_GAP),
  }
}

export function linkGeometry(from: FlowNode, to: FlowNode, vertical: boolean): LinkGeometry {
  const a = nodePosition(from, vertical)
  const b = nodePosition(to, vertical)
  const start = vertical
    ? { x: a.x + NODE_WIDTH / 2, y: a.y + NODE_HEIGHT }
    : { x: a.x + NODE_WIDTH, y: a.y + NODE_HEIGHT / 2 }
  const end = vertical ? { x: b.x + NODE_WIDTH / 2, y: b.y } : { x: b.x, y: b.y + NODE_HEIGHT / 2 }
  const mid = { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2 }
  const first = vertical ? { x: start.x, y: mid.y } : { x: mid.x, y: start.y }
  const second = vertical ? { x: end.x, y: mid.y } : { x: mid.x, y: end.y }

  return {
    path: `M ${start.x} ${start.y} C ${first.x} ${first.y}, ${second.x} ${second.y}, ${end.x} ${end.y}`,
    mid,
  }
}
