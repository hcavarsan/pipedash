import { Button, Group, Stack } from '@mantine/core'
import type { ReactNode } from 'react'
import { SubmitButton } from './SubmitButton'

export interface WizardAction {
  label: string
  submit?: boolean
  intent?: string
  disabled?: boolean
  loading?: boolean
  onClick?: () => void
}

interface DesktopFooterProps {
  primary: WizardAction | undefined
  secondary?: WizardAction | undefined
  leading?: ReactNode
  formId: string
  busy: boolean
  onCancel?: (() => void) | undefined
  onBack?: (() => void) | undefined
  fullWidth: boolean
}

interface ActionButtonProps {
  action: WizardAction
  variant: 'filled' | 'default'
  formId: string
  busy: boolean
  fullWidth: boolean
}

function ActionButton({ action, variant, formId, busy, fullWidth }: ActionButtonProps): ReactNode {
  const submitProps = {
    type: 'submit',
    form: formId,
    ...(action.intent ? { name: 'intent', value: action.intent } : {}),
  } as const

  return (
    <Button
      size="sm"
      variant={variant}
      fullWidth={fullWidth}
      loading={action.loading === true}
      disabled={busy || action.disabled === true}
      {...(action.submit ? submitProps : { onClick: action.onClick })}
    >
      {action.label}
    </Button>
  )
}

export function DesktopFooter({
  primary,
  secondary,
  leading,
  formId,
  busy,
  onCancel,
  onBack,
  fullWidth,
}: DesktopFooterProps): ReactNode {
  const groupProps = fullWidth ? { grow: true, style: { flex: '1 1 100%' } } : {}

  return (
    <Group justify="space-between" gap="sm">
      <Group gap="sm" {...groupProps}>
        {onBack && (
          <Button variant="default" size="sm" onClick={onBack} disabled={busy} fullWidth={fullWidth}>
            Back
          </Button>
        )}
        {leading}
      </Group>

      <Group gap="sm" {...groupProps}>
        {onCancel && (
          <Button variant="default" size="sm" onClick={onCancel} disabled={busy} fullWidth={fullWidth}>
            Cancel
          </Button>
        )}
        {secondary && (
          <ActionButton action={secondary} variant="default" formId={formId} busy={busy} fullWidth={fullWidth} />
        )}
        {primary && (
          <ActionButton
            key={primary.submit ? 'submit' : 'next'}
            action={primary}
            variant="filled"
            formId={formId}
            busy={busy}
            fullWidth={fullWidth}
          />
        )}
      </Group>
    </Group>
  )
}

interface ShellFooterProps {
  primary: WizardAction | undefined
  busy: boolean
  onBack: (() => void) | undefined
}

export function ShellFooter({ primary, busy, onBack }: ShellFooterProps): ReactNode {
  return (
    <Stack gap="3xs">
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
            loading={primary.loading === true}
            disabled={busy || primary.disabled === true}
          >
            {primary.label}
          </Button>
        ))}
      {onBack && (
        <Button size="md" fullWidth variant="subtle" onClick={onBack} disabled={busy}>
          Back
        </Button>
      )}
    </Stack>
  )
}
