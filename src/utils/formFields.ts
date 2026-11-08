import type { InputHTMLAttributes } from 'react'

export type FieldAttributes = Pick<
  InputHTMLAttributes<HTMLInputElement>,
  'type' | 'inputMode' | 'autoCapitalize' | 'autoCorrect' | 'autoComplete' | 'enterKeyHint' | 'spellCheck'
>

export type FieldInputKind = 'text' | 'password' | 'number' | 'textarea'

interface FieldSchema {
  name: string
  label?: string | null
  kind: FieldInputKind
  defaultValue?: unknown
  last?: boolean
}

const URL_HINT = /\b(url|uri|endpoint|host|domain)\b|_(url|uri|host)\b/i
const EMAIL_HINT = /e-?mail/i
const USERNAME_HINT = /\buser(name)?\b|^user(name)?$|_user(name)?$/i
const ONE_TIME_HINT = /one.?time|\b[to]?otp\b|2fa|mfa/i
const PASSWORD_HINT = /pass(word|phrase)/i
const PROSE_HINT = /message|description|title|comment|note|reason|summary/i

const TECHNICAL = { autoCapitalize: 'off', autoCorrect: 'off', spellCheck: false } as const
const PROSE = { autoCapitalize: 'sentences', autoCorrect: 'on', spellCheck: true } as const

const isFractional = (value: unknown): boolean => {
  const parsed = Number(value)

  return Number.isFinite(parsed) && !Number.isInteger(parsed)
}

const passwordAutoComplete = (hint: string): 'one-time-code' | 'current-password' | 'off' => {
  if (ONE_TIME_HINT.test(hint)) {
    return 'one-time-code'
  }
  if (PASSWORD_HINT.test(hint)) {
    return 'current-password'
  }

  return 'off'
}

export function fieldAttributes({ name, label, kind, defaultValue, last = false }: FieldSchema): FieldAttributes {
  const hint = `${name} ${label ?? ''}`
  const returnKeyHint = last ? 'go' : 'next'
  const enterKeyHint = kind === 'textarea' ? undefined : returnKeyHint

  if (kind === 'number') {
    return { inputMode: isFractional(defaultValue) ? 'decimal' : 'numeric', autoComplete: 'off', enterKeyHint }
  }

  if (kind === 'password') {
    const autoComplete = passwordAutoComplete(hint)

    return { ...TECHNICAL, autoComplete, enterKeyHint }
  }

  if (EMAIL_HINT.test(hint)) {
    return { ...TECHNICAL, type: 'email', inputMode: 'email', autoComplete: 'email', enterKeyHint }
  }

  if (URL_HINT.test(hint)) {
    return { ...TECHNICAL, type: 'url', inputMode: 'url', autoComplete: 'off', enterKeyHint }
  }

  if (USERNAME_HINT.test(hint)) {
    return { ...TECHNICAL, autoComplete: 'username', enterKeyHint }
  }

  if (PROSE_HINT.test(hint)) {
    return { ...PROSE, autoComplete: 'off', enterKeyHint }
  }

  return { ...TECHNICAL, autoComplete: 'off', enterKeyHint }
}

export const helpId = (id: string): string => `${id}-help`

export const errorId = (id: string): string => `${id}-error`

interface FieldState {
  required?: boolean | undefined
  error?: string | null | undefined
  description?: string | null | undefined
}

interface FieldA11y {
  required?: true
  'aria-required'?: true
  'aria-invalid': boolean
  'aria-describedby'?: string
  'aria-errormessage'?: string
}

export function fieldA11y(id: string, { required, error, description }: FieldState): FieldA11y {
  const describedBy = [description ? helpId(id) : null, error ? errorId(id) : null].filter(Boolean).join(' ')

  return {
    ...(required ? { required: true, 'aria-required': true } : {}),
    'aria-invalid': Boolean(error),
    ...(describedBy ? { 'aria-describedby': describedBy } : {}),
    ...(error ? { 'aria-errormessage': errorId(id) } : {}),
  }
}

export const withoutType = ({ type, ...rest }: FieldAttributes): Omit<FieldAttributes, 'type'> => rest
