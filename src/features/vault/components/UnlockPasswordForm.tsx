import { Divider, PasswordInput, Stack } from '@mantine/core'
import { type ChangeEvent, type KeyboardEvent, type ReactNode, useActionState, useRef, useState } from 'react'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { SubmitButton } from '../../../components/forms/SubmitButton'
import { useBlurValidation } from '../../../hooks/useBlurValidation'
import { useUnlockVault } from '../../../queries/useVaultQueries'
import { errorMessage } from '../../../utils/errorDisplay'

type FieldKey = 'password' | 'confirmation'

interface UnlockActionState {
  globalError: string | null
}

interface UnlockPasswordFormProps {
  firstTime: boolean
  mobileShell: boolean
  disabled: boolean
  biometricEnabled: boolean
  onSuccess: () => void
}

const INITIAL_STATE: UnlockActionState = { globalError: null }
const UNLOCK_FAILED = "Couldn't unlock the vault. Check the password and try again."
const PASSWORD_REQUIRED = 'Password is required'
const PASSWORDS_MISMATCH = 'Passwords do not match'

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
  const [dismissed, setDismissed] = useState<UnlockActionState | null>(null)
  const confirmRef = useRef<HTMLInputElement>(null)
  const passwordRef = useRef('')
  const { mutateAsync } = useUnlockVault()
  const validation = useBlurValidation<FieldKey>((key, value) => {
    if (key === 'password') {
      return value.trim() ? undefined : PASSWORD_REQUIRED
    }

    return firstTime && value !== passwordRef.current ? PASSWORDS_MISMATCH : undefined
  })

  const unlockAction = async (_previous: UnlockActionState, formData: FormData): Promise<UnlockActionState> => {
    const submitted = fieldValue(formData, 'password')
    passwordRef.current = submitted

    if (!validation.validateAll({ password: submitted, confirmation: fieldValue(formData, 'confirmation') })) {
      return INITIAL_STATE
    }

    try {
      const result = await mutateAsync(submitted)

      if (!result.success) {
        return { globalError: result.message || UNLOCK_FAILED }
      }

      setPassword('')
      setConfirmation('')
      passwordRef.current = ''
      validation.reset()
      onSuccess()

      return INITIAL_STATE
    } catch (err) {
      return { globalError: errorMessage(err, UNLOCK_FAILED) }
    }
  }

  const [state, formAction] = useActionState(unlockAction, INITIAL_STATE)
  const visible = state === dismissed ? INITIAL_STATE : state
  const globalOnPassword = mobileShell && visible.globalError && !confirmation ? visible.globalError : null
  const globalOnConfirmation = mobileShell && visible.globalError && confirmation ? visible.globalError : null
  const passwordError = validation.errors.password ?? globalOnPassword
  const confirmationError = validation.errors.confirmation ?? globalOnConfirmation

  const handlePasswordChange = (event: ChangeEvent<HTMLInputElement>): void => {
    const next = event.currentTarget.value
    passwordRef.current = next
    setPassword(next)
    setDismissed(state)
    validation.onChange('password', next)
    validation.onChange('confirmation', confirmation)
  }

  const handleConfirmationChange = (event: ChangeEvent<HTMLInputElement>): void => {
    const next = event.currentTarget.value
    setConfirmation(next)
    setDismissed(state)
    validation.onChange('confirmation', next)
  }

  const handlePasswordKeyDown = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (mobileShell && firstTime && event.key === 'Enter' && !event.nativeEvent.isComposing) {
      event.preventDefault()
      confirmRef.current?.focus()
    }
  }

  const confirmationLabels = mobileShell
    ? { 'aria-label': 'Confirm password', placeholder: 'Confirm password' }
    : { label: 'Confirm password', placeholder: 'Re-enter your password' }

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
            onChange={handlePasswordChange}
            onBlur={() => validation.onBlur('password', password)}
            onKeyDown={handlePasswordKeyDown}
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
              {...confirmationLabels}
              ref={confirmRef}
              name="confirmation"
              required
              value={confirmation}
              onChange={handleConfirmationChange}
              onBlur={() => validation.onBlur('confirmation', confirmation)}
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
