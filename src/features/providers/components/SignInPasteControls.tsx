import { Button, Group, Stack, TextInput } from '@mantine/core'
import type { KeyboardEvent, ReactNode } from 'react'

export const PASTE_LABEL = 'Address of the page after sign-in'
export const PASTE_DESCRIPTION =
  "After you sign in, the browser opens a page that can't load. Copy its address from the address bar and paste it here."

interface SignInPasteControlsProps {
  value: string
  error: string | undefined
  pending: boolean
  shell?: boolean
  onChange: (value: string) => void
  onSubmit: () => void
}

export function SignInPasteControls({
  value,
  error,
  pending,
  shell = false,
  onChange,
  onSubmit,
}: SignInPasteControlsProps): ReactNode {
  const handleKey = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === 'Enter') {
      event.preventDefault()
      onSubmit()
    }
  }

  const input = (
    <TextInput
      {...(shell ? { variant: 'unstyled' } : {})}
      label={shell ? undefined : PASTE_LABEL}
      description={shell ? undefined : PASTE_DESCRIPTION}
      aria-label={PASTE_LABEL}
      error={shell ? undefined : error}
      value={value}
      onChange={(event) => onChange(event.currentTarget.value)}
      onKeyDown={handleKey}
      autoComplete="off"
      autoCapitalize="off"
      autoCorrect="off"
      spellCheck={false}
      inputMode="url"
      enterKeyHint="go"
    />
  )

  if (shell) {
    return input
  }

  return (
    <Stack gap="xs">
      {input}
      <Group>
        <Button size="xs" onClick={onSubmit} loading={pending} disabled={!value.trim()}>
          Continue
        </Button>
      </Group>
    </Stack>
  )
}
