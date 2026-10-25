import { fullSwipeArmed, type RowSwipeResult } from '../../lib/gestures'

export const DISARM_HYSTERESIS = 24

export function nextArmed(armed: boolean, position: number, rowWidth: number): boolean {
  return armed ? fullSwipeArmed(position + DISARM_HYSTERESIS, rowWidth) : fullSwipeArmed(position, rowWidth)
}

export function settleDecision(decision: RowSwipeResult, armed: boolean): RowSwipeResult {
  return decision === 'open' && armed ? 'full' : decision
}
