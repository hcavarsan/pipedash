import { NativeSelect, type NativeSelectProps } from '@mantine/core'
import type { ReactNode } from 'react'

import { IosRow } from '../ios/IosList'

import classes from './metrics.module.css'

type MetricsPickerRowProps = Omit<NativeSelectProps, 'label' | 'variant' | 'onChange'> & {
  label: string
  onChange: (value: string) => void
}

export const MetricsPickerRow = ({ label, onChange, ...selectProps }: MetricsPickerRowProps): ReactNode => (
  <IosRow
    title={label}
    accessory={
      <NativeSelect
        variant="unstyled"
        aria-label={label}
        rightSectionWidth={24}
        onChange={(event) => onChange(event.currentTarget.value)}
        classNames={{
          root: classes.pickerRoot,
          wrapper: classes.pickerWrapper,
          input: classes.pickerInput,
          section: classes.pickerSection,
        }}
        {...selectProps}
      />
    }
  />
)
