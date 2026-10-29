import { Button, Group, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { openConfirm } from '../../../utils/confirm'
import { SubmitButton } from '../../forms/SubmitButton'

export interface WizardAction {
  label: string
  submit?: boolean
  disabled?: boolean
  onClick?: () => void
}

interface DesktopFooterProps {
  primary: WizardAction | undefined
  formId: string
  isPending: boolean
  onCancel?: (() => void) | undefined
  onBack?: (() => void) | undefined
  fullWidth: boolean
}

export function DesktopFooter({
  primary,
  formId,
  isPending,
  onCancel,
  onBack,
  fullWidth,
}: DesktopFooterProps): ReactNode {
  return (
    <Group justify="flex-end" gap="sm">
      {onCancel && (
        <Button variant="default" size="sm" onClick={onCancel} disabled={isPending} fullWidth={fullWidth}>
          Cancel
        </Button>
      )}
      {onBack && (
        <Button variant="default" size="sm" onClick={onBack} disabled={isPending} fullWidth={fullWidth}>
          Back
        </Button>
      )}
      {primary && (
        <Button
          size="sm"
          fullWidth={fullWidth}
          loading={isPending}
          disabled={primary.disabled === true}
          {...(primary.submit ? { type: 'submit', form: formId } : { onClick: primary.onClick })}
        >
          {primary.label}
        </Button>
      )}
    </Group>
  )
}

interface ShellFooterProps {
  primary: WizardAction | undefined
  isPending: boolean
  onBack: (() => void) | undefined
}

export function ShellFooter({ primary, isPending, onBack }: ShellFooterProps): ReactNode {
  return (
    <Stack gap={4}>
      {primary &&
        (primary.submit ? (
          <SubmitButton size="lg" fullWidth disabled={primary.disabled === true}>
            {primary.label}
          </SubmitButton>
        ) : (
          <Button
            size="lg"
            fullWidth
            onClick={primary.onClick}
            loading={isPending}
            disabled={primary.disabled === true}
          >
            {primary.label}
          </Button>
        ))}
      {onBack && (
        <Button size="md" fullWidth variant="subtle" onClick={onBack} disabled={isPending}>
          Back
        </Button>
      )}
    </Stack>
  )
}

interface SkipTransferChanges {
  databaseBackend: boolean
  credentialStorage: boolean
  dataDirectory: boolean
}

export function confirmSkipTransfer(changed: SkipTransferChanges, onConfirm: () => void): void {
  const changes = []

  if (changed.databaseBackend) {
    changes.push('database backend')
  }
  if (changed.credentialStorage) {
    changes.push('credential storage (keyring → encrypted)')
  }
  if (changed.dataDirectory) {
    changes.push('data directory')
  }

  openConfirm({
    title: 'Skip data transfer?',
    centered: true,
    zIndex: 400,
    children: (
      <Text size="sm">
        You are changing <strong>{changes.join(' and ')}</strong> without transferring data. Existing providers,
        credentials and history stay in the previous location and won't be available after switching.
      </Text>
    ),
    labels: {
      confirm: 'Skip data transfer',
      cancel: 'Go back',
    },
    confirmProps: { color: 'red' },
    onConfirm,
  })
}
