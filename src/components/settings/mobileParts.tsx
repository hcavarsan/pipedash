import {
  NativeSelect,
  type NativeSelectProps,
  NumberInput,
  type NumberInputProps,
  Switch,
  type SwitchProps,
  Text,
  TextInput,
} from '@mantine/core'
import { type ComponentProps, type ReactNode, useId } from 'react'

import { IosRow } from '../ios/IosList'

import classes from './mobileParts.module.css'

export const AccentText = ({ children }: { children: ReactNode }) => (
  <Text span inherit c="var(--ios-tint)">
    {children}
  </Text>
)

export const RowSelect = ({
  w = '9em',
  onChange,
  ...props
}: Omit<NativeSelectProps, 'onChange'> & { onChange?: (value: string | null) => void }) => (
  <NativeSelect
    variant="unstyled"
    w={w}
    rightSectionWidth={20}
    classNames={{ root: classes.field, input: `${classes.fieldInput} ${classes.fieldSelect}` }}
    onChange={(event) => onChange?.(event.currentTarget.value)}
    {...props}
  />
)

export const RowNumberInput = ({ w = '6em', ...props }: NumberInputProps) => (
  <NumberInput
    variant="unstyled"
    hideControls
    inputMode="numeric"
    w={w}
    classNames={{ root: classes.field, input: classes.fieldInput }}
    {...props}
  />
)

export const RowTextInput = ({ w = '12em', ...props }: ComponentProps<typeof TextInput>) => (
  <TextInput
    variant="unstyled"
    autoCapitalize="none"
    autoCorrect="off"
    spellCheck={false}
    w={w}
    classNames={{ root: classes.field, input: classes.fieldInput }}
    {...props}
  />
)

interface LabeledRowProps {
  title: ReactNode
  subtitle?: ReactNode
}

export const FieldRow = ({
  title,
  subtitle,
  children,
}: LabeledRowProps & { children: (fieldId: string) => ReactNode }) => {
  const id = useId()

  return (
    <IosRow
      title={
        <label htmlFor={id} className={classes.rowLabel}>
          {title}
        </label>
      }
      subtitle={subtitle}
      accessory={children(id)}
    />
  )
}

export const SwitchRow = ({ title, subtitle, ...props }: LabeledRowProps & Omit<SwitchProps, 'id' | 'label'>) => {
  const id = useId()
  const descriptionId = `${id}-description`

  return (
    <IosRow
      title={
        <label htmlFor={id} className={classes.rowLabel}>
          {title}
        </label>
      }
      subtitle={subtitle && <span id={descriptionId}>{subtitle}</span>}
      accessory={<Switch id={id} aria-describedby={subtitle ? descriptionId : undefined} {...props} />}
    />
  )
}
