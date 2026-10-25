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

export function fieldAttributes({ name, label, kind, defaultValue, last = false }: FieldSchema): FieldAttributes {
  const hint = `${name} ${label ?? ''}`
  const enterKeyHint = kind === 'textarea' ? undefined : last ? 'go' : 'next'

  if (kind === 'number') {
    return { inputMode: isFractional(defaultValue) ? 'decimal' : 'numeric', autoComplete: 'off', enterKeyHint }
  }

  if (kind === 'password') {
    const autoComplete = ONE_TIME_HINT.test(hint)
      ? 'one-time-code'
      : PASSWORD_HINT.test(hint)
        ? 'current-password'
        : 'off'

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

export const helpId = (id: string) => `${id}-help`

export const errorId = (id: string) => `${id}-error`

interface FieldState {
  required?: boolean
  error?: string | null
  description?: string | null
}

export function fieldA11y(id: string, { required, error, description }: FieldState) {
  const describedBy = [description ? helpId(id) : null, error ? errorId(id) : null].filter(Boolean).join(' ')

  return {
    required: required || undefined,
    'aria-required': required || undefined,
    'aria-invalid': Boolean(error),
    'aria-describedby': describedBy || undefined,
    'aria-errormessage': error ? errorId(id) : undefined,
  }
}

export const withoutType = ({ type, ...rest }: FieldAttributes) => rest
