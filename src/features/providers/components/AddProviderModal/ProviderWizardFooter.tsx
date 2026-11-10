import { Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { DesktopFooter, type WizardAction } from '../../../../components/forms/WizardFooter'
import type { ProviderSubmitIntent, Step } from './types'

interface ProviderWizardFooterProps {
  step: Step
  formId: string
  editMode: boolean
  saveOnly: boolean
  selectionNeedsRefresh: boolean
  providerSelected: boolean
  providerNameValid: boolean
  selectedCount: number
  selectionRequired: boolean
  selectingAll: boolean
  items: string
  permissionRow: ReactNode
  busy: boolean
  pendingIntent: ProviderSubmitIntent | null
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
  providerSelected,
  providerNameValid,
  selectedCount,
  selectionRequired,
  selectingAll,
  items,
  permissionRow,
  busy,
  pendingIntent,
  onBack,
  onCancel,
  onSaveSelection,
}: ProviderWizardFooterProps): ReactNode {
  const detailsBlocked = !providerSelected || !providerNameValid
  const submitAction = (intent: ProviderSubmitIntent, label: string, disabled: boolean): WizardAction => ({
    label,
    submit: true,
    intent,
    disabled,
    loading: pendingIntent === intent,
  })

  if (step === 'pipelines') {
    return (
      <DesktopFooter
        primary={{
          label: editMode ? 'Update provider' : 'Add provider',
          onClick: onSaveSelection,
          disabled: (selectionRequired && selectedCount === 0) || selectingAll,
          loading: pendingIntent === 'save-selection',
        }}
        leading={
          selectedCount > 0 && (
            <Text size="sm" c="dimmed">
              {selectedCount} selected
            </Text>
          )
        }
        formId={formId}
        busy={busy}
        onCancel={onCancel}
        onBack={onBack}
        fullWidth={false}
      />
    )
  }

  const onCredentials = step === 'credentials'
  const leading = onCredentials && (
    <>
      {permissionRow}
      {selectionNeedsRefresh && (
        <Text size="xs" c="dimmed">
          Select {items} again after changing connection scope.
        </Text>
      )}
    </>
  )

  return (
    <DesktopFooter
      primary={submitAction('continue', 'Continue', detailsBlocked)}
      secondary={
        editMode
          ? submitAction(
              'save-credentials',
              'Save',
              detailsBlocked || (onCredentials ? selectionNeedsRefresh : !saveOnly)
            )
          : undefined
      }
      leading={leading}
      formId={formId}
      busy={busy}
      onCancel={onCancel}
      onBack={onCredentials ? onBack : undefined}
      fullWidth={false}
    />
  )
}
