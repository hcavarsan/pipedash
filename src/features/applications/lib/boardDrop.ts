import type { ApplicationEdit, Stage } from '../../../types/applications'

export type DragItem =
  | { kind: 'pipeline'; label: string; pipelineId: string; applicationId: number | null; stage: Stage | null }
  | { kind: 'target'; label: string; targetKey: string; applicationId: number; groupKey: string }

export type DropZone =
  | { kind: 'unassigned' }
  | { kind: 'new_application' }
  | { kind: 'lane'; applicationId: number; stage: Stage }
  | { kind: 'environment'; applicationId: number; groupKey: string; label: string; targetKeys: string[] }
  | { kind: 'new_environment'; applicationId: number }

export type DropAction =
  | { kind: 'edit'; applicationId: number; edit: ApplicationEdit }
  | { kind: 'create_application'; pipelineId: string }
  | { kind: 'create_environment'; applicationId: number; targetKey: string }

function pipelineDrop(item: Extract<DragItem, { kind: 'pipeline' }>, zone: DropZone): DropAction | null {
  const { pipelineId, applicationId, stage } = item

  if (zone.kind === 'new_application') {
    return { kind: 'create_application', pipelineId }
  }
  if (zone.kind === 'unassigned' && applicationId !== null) {
    return { kind: 'edit', applicationId, edit: { type: 'unlink', pipeline_id: pipelineId } }
  }
  if (zone.kind !== 'lane') {
    return null
  }
  if (zone.applicationId !== applicationId) {
    return {
      kind: 'edit',
      applicationId: zone.applicationId,
      edit: { type: 'link', pipeline_id: pipelineId, stage: zone.stage },
    }
  }
  if (zone.stage === stage) {
    return null
  }

  return {
    kind: 'edit',
    applicationId: zone.applicationId,
    edit: { type: 'stage', pipeline_id: pipelineId, stage: zone.stage },
  }
}

function targetDrop(item: Extract<DragItem, { kind: 'target' }>, zone: DropZone): DropAction | null {
  const { targetKey, applicationId, groupKey } = item

  if (zone.kind === 'new_environment' && zone.applicationId === applicationId) {
    return { kind: 'create_environment', applicationId, targetKey }
  }
  if (zone.kind !== 'environment' || zone.applicationId !== applicationId || zone.groupKey === groupKey) {
    return null
  }

  return {
    kind: 'edit',
    applicationId,
    edit: { type: 'target_group', target_keys: [targetKey, ...zone.targetKeys], group_name: zone.label },
  }
}

export function dropAction(item: DragItem, zone: DropZone): DropAction | null {
  return item.kind === 'pipeline' ? pipelineDrop(item, zone) : targetDrop(item, zone)
}

export function zoneId(zone: DropZone): string {
  switch (zone.kind) {
    case 'lane':
      return `lane:${zone.applicationId}:${zone.stage}`
    case 'environment':
      return `environment:${zone.applicationId}:${zone.groupKey}`
    case 'new_environment':
      return `new_environment:${zone.applicationId}`
    default:
      return zone.kind
  }
}
