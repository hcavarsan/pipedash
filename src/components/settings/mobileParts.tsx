import {
  NumberInput,
  type NumberInputProps,
  Select,
  type SelectProps,
  Text,
  TextInput,
  type TextInputProps,
} from '@mantine/core'
import type { ReactNode } from 'react'

import classes from './mobileParts.module.css'

export const AccentText = ({ children }: { children: ReactNode }) => (
  <Text span inherit c="var(--mantine-color-blue-light-color)">
    {children}
  </Text>
)

export const RowSelect = ({ w = 140, ...props }: SelectProps) => (
  <Select
    variant="unstyled"
    allowDeselect={false}
    w={w}
    rightSectionWidth={20}
    comboboxProps={{ width: '100%' }}
    classNames={{ root: classes.field, input: `${classes.fieldInput} ${classes.fieldSelect}` }}
    {...props}
  />
)

export const RowNumberInput = ({ w = 90, ...props }: NumberInputProps) => (
  <NumberInput
    variant="unstyled"
    hideControls
    inputMode="numeric"
    w={w}
    classNames={{ root: classes.field, input: classes.fieldInput }}
    {...props}
  />
)

export const RowTextInput = ({ w = 200, ...props }: TextInputProps) => (
  <TextInput
    variant="unstyled"
    autoCapitalize="none"
    autoCorrect="off"
    w={w}
    classNames={{ root: classes.field, input: classes.fieldInput }}
    {...props}
  />
)
