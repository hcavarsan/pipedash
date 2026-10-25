import { Select, type SelectProps } from '@mantine/core'

import { IosRow } from '../ios/IosList'

import classes from './metrics.module.css'

type MetricsPickerRowProps = Omit<SelectProps, 'label' | 'variant'> & { label: string }

export const MetricsPickerRow = ({ label, ...selectProps }: MetricsPickerRowProps) => (
  <IosRow
    title={label}
    accessory={
      <Select
        variant="unstyled"
        aria-label={label}
        allowDeselect={false}
        rightSectionWidth={24}
        comboboxProps={{ width: '100%' }}
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
