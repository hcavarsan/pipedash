import { Button, Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'

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
  items: string
  permissionRow: ReactNode
  isPending: boolean
  onBack: () => void
  onCancel: () => void
  onSaveCredentials: () => void
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
  items,
  permissionRow,
  isPending,
  onBack,
  onCancel,
  onSaveCredentials,
  onSaveSelection,
}: ProviderWizardFooterProps): ReactNode {
  const cancel = (
    <Button variant="default" size="sm" onClick={onCancel} disabled={isPending}>
      Cancel
    </Button>
  )
  const back = (
    <Button variant="default" size="sm" onClick={onBack} disabled={isPending}>
      Back
    </Button>
  )
  const detailsBlocked = !providerSelected || !providerNameValid

  if (step === 'provider') {
    return (
      <Group justify="flex-end" gap="xs">
        {cancel}
        {editMode && (
          <Button size="sm" onClick={onSaveCredentials} loading={isPending} disabled={detailsBlocked || !saveOnly}>
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
          disabled={detailsBlocked || isPending}
        >
          {isMobile ? 'Next' : 'Next: credentials'}
        </Button>
      </Group>
    )
  }

  if (step === 'credentials') {
    return (
      <Group justify="space-between" gap="xs">
        <Group gap="xs">
          {back}
          {permissionRow}
          {selectionNeedsRefresh && (
            <Text size="xs" c="dimmed">
              Select {items} again after changing connection scope.
            </Text>
          )}
        </Group>

        <Group gap="xs">
          {cancel}
          {editMode && (
            <Button
              size="sm"
              type={saveOnly ? 'submit' : 'button'}
              form={formId}
              name="intent"
              value="save-credentials"
              loading={isPending}
              disabled={detailsBlocked || selectionNeedsRefresh}
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
            disabled={detailsBlocked || isPending}
          >
            {isMobile ? 'Next' : `Next: select ${items}`}
          </Button>
        </Group>
      </Group>
    )
  }

  const submitLabel = editMode ? 'Update' : 'Add'

  return (
    <Group justify="space-between">
      <Group gap="sm">
        {back}
        {selectedCount > 0 && (
          <Text size="sm" c="dimmed">
            {selectedCount} selected
          </Text>
        )}
      </Group>
      <Group gap="xs">
        {cancel}
        <Button size="sm" onClick={onSaveSelection} loading={isPending} disabled={selectedCount === 0 || selectingAll}>
          {isMobile ? submitLabel : `${submitLabel} provider`}
        </Button>
      </Group>
    </Group>
  )
}
