import { useEffect, useState } from 'react'

import { DEBOUNCE_DELAYS } from '../constants/intervals'

export function useDebounce<T>(value: T, delayMs = DEBOUNCE_DELAYS.FILTER): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value)

  useEffect(() => {
    const timeoutId = setTimeout(() => {
      setDebouncedValue(value)
    }, delayMs)

    return () => {
      clearTimeout(timeoutId)
    }
  }, [value, delayMs])

  return debouncedValue
}
