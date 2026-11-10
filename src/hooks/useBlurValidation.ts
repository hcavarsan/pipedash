import { useState } from 'react'

type Errors<K extends string> = Partial<Record<K, string>>

interface BlurValidation<K extends string> {
  errors: Errors<K>
  onBlur: (key: K, value: string) => void
  onChange: (key: K, value: string) => void
  validateAll: (values: Record<K, string>) => boolean
  reset: () => void
}

export function useBlurValidation<K extends string>(
  validate: (key: K, value: string) => string | undefined
): BlurValidation<K> {
  const [errors, setErrors] = useState<Errors<K>>({})

  const onBlur = (key: K, value: string): void => {
    setErrors((current) => ({ ...current, [key]: validate(key, value) }))
  }

  const onChange = (key: K, value: string): void => {
    setErrors((current) => (current[key] === undefined ? current : { ...current, [key]: validate(key, value) }))
  }

  const validateAll = (values: Record<K, string>): boolean => {
    const next: Errors<K> = {}
    let valid = true

    for (const key in values) {
      const error = validate(key, values[key])

      if (error !== undefined) {
        next[key] = error
        valid = false
      }
    }

    setErrors(next)

    return valid
  }

  const reset = (): void => setErrors({})

  return { errors, onBlur, onChange, validateAll, reset }
}
