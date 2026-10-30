import { Button, Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { PipelineLabels } from '../../../../utils/pipelinePresentation'

import type { Step } from './types'

interface ProviderWizardFooterProps {
  step: Step
  formId: string
  editMode: boolean
  saveOnly: boolean
  selectionNeedsRefresh: boolean
  isMobile: boolean
  providerSelected: boolean
  providerNameValid: boolean
  selectedCount: number
  selectingAll: boolean
  labels: PipelineLabels
  permissionRow: ReactNode
  isPending: boolean
  onBack: () => void
  onCancel: () => void
  onSaveSelection: () => void
}

export function ProviderWizardFooter({
  step,
  formId,
  editMode,
  saveOnly,
  selectionNeedsRefresh,
  isMobile,
  providerSelected,
  providerNameValid,
  selectedCount,
  selectingAll,
  labels,
  permissionRow,
  isPending,
  onBack,
  onCancel,
  onSaveSelection,
}: ProviderWizardFooterProps): ReactNode {
  if (step === 'credentials') {
    return (
      <Group justify="space-between" gap="xs">
        <Group gap="xs">
          {permissionRow}
          {selectionNeedsRefresh && (
            <Text size="xs" c="dimmed">
              Select {labels.items} again after changing connection scope.
            </Text>
          )}
        </Group>

        <Group gap="xs">
          <Button variant="default" size="sm" onClick={onCancel} disabled={isPending}>
            Cancel
          </Button>
          {editMode && (
            <Button
              size="sm"
              type={saveOnly ? 'submit' : 'button'}
              form={formId}
              name="intent"
              value="save-credentials"
              loading={isPending}
              disabled={!providerSelected || !providerNameValid || selectionNeedsRefresh}
            >
              Save
            </Button>
          )}
          <Button
            variant={editMode ? 'default' : 'filled'}
            size="sm"
            type="submit"
            form={formId}
            name="intent"
            value="continue"
            loading={isPending && !editMode}
            disabled={!providerSelected || !providerNameValid || isPending}
          >
            {isMobile ? 'Next' : `Next: select ${labels.items}`}
          </Button>
        </Group>
      </Group>
    )
  }

  return (
    <Group justify="space-between">
      <Group gap="sm">
        <Button variant="default" size="sm" onClick={onBack} disabled={isPending}>
          Back
        </Button>
        {selectedCount > 0 && (
          <Text size="sm" c="dimmed">
            {selectedCount} selected
          </Text>
        )}
      </Group>
      <Group gap="xs">
        <Button variant="default" size="sm" onClick={onCancel} disabled={isPending}>
          Cancel
        </Button>
        <Button size="sm" onClick={onSaveSelection} loading={isPending} disabled={selectedCount === 0 || selectingAll}>
          {editMode ? (isMobile ? 'Update' : 'Update provider') : isMobile ? 'Add' : 'Add provider'}
        </Button>
      </Group>
    </Group>
  )
}
