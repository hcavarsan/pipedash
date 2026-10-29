import { Divider, PasswordInput, Stack } from '@mantine/core'
import { type ReactNode, useActionState, useRef, useState } from 'react'

import { useUnlockVault } from '../../queries/useVaultQueries'
import { errorMessage } from '../../utils/errorDisplay'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { SubmitButton } from '../forms/SubmitButton'

interface UnlockActionState {
  passwordError: string | null
  confirmationError: string | null
  globalError: string | null
}

interface UnlockPasswordFormProps {
  firstTime: boolean
  mobileShell: boolean
  disabled: boolean
  biometricEnabled: boolean
  onSuccess: () => void
}

const INITIAL_STATE: UnlockActionState = { passwordError: null, confirmationError: null, globalError: null }
const UNLOCK_FAILED = "Couldn't unlock the vault. Check the password and try again."

function fieldValue(formData: FormData, name: string): string {
  const value = formData.get(name)

  return typeof value === 'string' ? value : ''
}

export function UnlockPasswordForm({
  firstTime,
  mobileShell,
  disabled,
  biometricEnabled,
  onSuccess,
}: UnlockPasswordFormProps): ReactNode {
  const [password, setPassword] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const [blurred, setBlurred] = useState({ password: false, confirmation: false })
  const [dismissed, setDismissed] = useState<UnlockActionState | null>(null)
  const confirmRef = useRef<HTMLInputElement>(null)
  const { mutateAsync } = useUnlockVault()

  const unlockAction = async (_previous: UnlockActionState, formData: FormData): Promise<UnlockActionState> => {
    setBlurred({ password: true, confirmation: true })

    const submitted = fieldValue(formData, 'password')

    if (!submitted.trim()) {
      return { ...INITIAL_STATE, passwordError: 'Password is required' }
    }

    if (firstTime && submitted !== fieldValue(formData, 'confirmation')) {
      return { ...INITIAL_STATE, confirmationError: 'Passwords do not match' }
    }

    try {
      const result = await mutateAsync(submitted)

      if (!result.success) {
        return { ...INITIAL_STATE, globalError: result.message || UNLOCK_FAILED }
      }

      setPassword('')
      setConfirmation('')
      onSuccess()

      return INITIAL_STATE
    } catch (err) {
      return { ...INITIAL_STATE, globalError: errorMessage(err, UNLOCK_FAILED) }
    }
  }

  const [state, formAction] = useActionState(unlockAction, INITIAL_STATE)
  const visible = state === dismissed ? INITIAL_STATE : state
  const dismissErrors = (): void => {
    if (!mobileShell) {
      setDismissed(state)
    }
  }
  const globalOnPassword = mobileShell && visible.globalError && !confirmation ? visible.globalError : null
  const globalOnConfirmation = mobileShell && visible.globalError && confirmation ? visible.globalError : null
  const blurPasswordError = !mobileShell && blurred.password && !password.trim() ? 'Password is required' : null
  const blurConfirmationError =
    !mobileShell && blurred.confirmation && password !== confirmation ? 'Passwords do not match' : null
  const passwordError = blurPasswordError ?? visible.passwordError ?? globalOnPassword
  const confirmationError = blurConfirmationError ?? visible.confirmationError ?? globalOnConfirmation

  return (
    <form noValidate action={formAction}>
      <Stack gap={mobileShell ? 'md' : 'lg'}>
        {!mobileShell && biometricEnabled && <Divider label="or use your password" />}

        {!mobileShell && (
          <FormErrorDisplay
            title="Couldn't unlock vault"
            globalError={visible.globalError}
            onDismiss={() => setDismissed(state)}
          />
        )}

        <Stack gap={mobileShell ? 'sm' : 'md'}>
          <PasswordInput
            {...(mobileShell
              ? { 'aria-label': 'Password', placeholder: 'Password' }
              : { label: 'Password', placeholder: 'Enter your password' })}
            name="password"
            required
            value={password}
            onChange={(event) => {
              setPassword(event.currentTarget.value)
              dismissErrors()
            }}
            onBlur={() => setBlurred((value) => ({ ...value, password: true }))}
            onKeyDown={(event) => {
              if (firstTime && event.key === 'Enter' && !event.nativeEvent.isComposing) {
                event.preventDefault()
                confirmRef.current?.focus()
              }
            }}
            disabled={disabled}
            error={passwordError}
            size="md"
            autoComplete={firstTime ? 'new-password' : 'current-password'}
            enterKeyHint={firstTime ? 'next' : 'go'}
            autoFocus={!biometricEnabled}
            data-autofocus
          />

          {firstTime && (
            <PasswordInput
              {...(mobileShell
                ? { 'aria-label': 'Confirm password', placeholder: 'Confirm password' }
                : { label: 'Confirm password', placeholder: 'Re-enter your password' })}
              ref={confirmRef}
              name="confirmation"
              required
              value={confirmation}
              onChange={(event) => {
                setConfirmation(event.currentTarget.value)
                dismissErrors()
              }}
              onBlur={() => setBlurred((value) => ({ ...value, confirmation: true }))}
              disabled={disabled}
              error={confirmationError}
              size="md"
              autoComplete="new-password"
              enterKeyHint="go"
            />
          )}
        </Stack>

        <SubmitButton fullWidth size={mobileShell ? 'lg' : 'md'} disabled={disabled}>
          Unlock
        </SubmitButton>
      </Stack>
    </form>
  )
}
