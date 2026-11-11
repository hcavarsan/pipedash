import type { ApplicationTarget } from '../../../types/applications'

export interface GroupOption {
  value: string
  label: string
}

export function groupOptions(targets: ApplicationTarget[]): GroupOption[] {
  return [...new Map(targets.map((target) => [target.group_key, target.group_label])).entries()].map(
    ([value, label]) => ({ value, label })
  )
}
