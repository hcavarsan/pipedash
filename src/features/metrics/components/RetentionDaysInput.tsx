import { NumberInput, type NumberInputProps } from '@mantine/core'
import type { KeyboardEvent, ReactNode } from 'react'
import { RowNumberInput } from '../../../components/ios/mobileParts'

interface RetentionDaysInputProps extends NumberInputProps {
  row?: boolean | undefined
  onEnter?: (() => void) | undefined
}

export function RetentionDaysInput({ row = false, onEnter, ...props }: RetentionDaysInputProps): ReactNode {
  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
      event.preventDefault()
      onEnter?.()
    }
  }

  const inputProps: NumberInputProps = {
    allowDecimal: false,
    clampBehavior: 'none',
    min: 1,
    ...(onEnter ? { onKeyDown: handleKeyDown } : {}),
    ...props,
  }

  return row ? <RowNumberInput {...inputProps} /> : <NumberInput {...inputProps} />
}
