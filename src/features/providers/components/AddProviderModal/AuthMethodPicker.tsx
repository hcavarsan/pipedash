import { SegmentedControl, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import styles from '../../../../components/forms/ShellForm.module.css'
import type { AuthMode, AuthOption } from '../../../../types/plugins'

const HEADING = 'How to connect'

interface AuthMethodPickerProps {
  options: AuthOption[]
  value: AuthMode
  disabled: boolean
  shell: boolean
  headingId: string
  onChange: (mode: AuthMode) => void
}

export function AuthMethodPicker({
  options,
  value,
  disabled,
  shell,
  headingId,
  onChange,
}: AuthMethodPickerProps): ReactNode {
  const description = options.find((option) => option.mode === value)?.description
  const control = (
    <SegmentedControl
      aria-label="Authentication method"
      fullWidth
      data={options.map((option) => ({ value: option.mode, label: option.label }))}
      value={value}
      disabled={disabled}
      onChange={(next) => {
        const mode = options.find((option) => option.mode === next)?.mode

        if (mode) {
          onChange(mode)
        }
      }}
    />
  )

  if (shell) {
    return (
      <>
        <h2 id={headingId} className={styles.sectionTitle}>
          {HEADING}
        </h2>
        {control}
        {description && <p className={styles.note}>{description}</p>}
      </>
    )
  }

  return (
    <Stack component="section" gap="sm" aria-labelledby={headingId}>
      <Text id={headingId} component="h3" size="sm" fw={600} m={0}>
        {HEADING}
      </Text>
      {control}
      {description && (
        <Text size="sm" c="dimmed">
          {description}
        </Text>
      )}
    </Stack>
  )
}
