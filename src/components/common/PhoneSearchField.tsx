import { IconCircleXFilled, IconSearch } from '@tabler/icons-react'
import { type ReactNode, useRef, useState } from 'react'

import classes from './PhoneSearchField.module.css'

interface PhoneSearchFieldProps {
  value: string
  placeholder?: string | undefined
  onChange: (value: string) => void
  onClear: () => void
  onSubmit?: (() => void) | undefined
  cancelable?: boolean | undefined
}

export function PhoneSearchField({
  value,
  placeholder = 'Search',
  onChange,
  onClear,
  onSubmit,
  cancelable = true,
}: PhoneSearchFieldProps): ReactNode {
  const inputRef = useRef<HTMLInputElement>(null)
  const [active, setActive] = useState(false)

  const cancel = (): void => {
    onClear()
    inputRef.current?.blur()
    setActive(false)
  }

  const submitSearch = (): void => {
    if (onSubmit) {
      onSubmit()

      return
    }

    inputRef.current?.blur()
  }

  return (
    <search className={classes.search}>
      <div className={classes.form}>
        <div className={classes.field}>
          <IconSearch className={classes.icon} size="1.1em" aria-hidden />
          <input
            name="search"
            ref={inputRef}
            type="search"
            className={classes.input}
            aria-label={placeholder}
            placeholder={placeholder}
            value={value}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            enterKeyHint="search"
            onChange={(event) => onChange(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                event.preventDefault()
                submitSearch()
              }
            }}
            onFocus={() => setActive(true)}
            onBlur={() => {
              if (!value) {
                setActive(false)
              }
            }}
          />
          {value && (
            <button
              type="button"
              className={classes.clear}
              aria-label="Clear search"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                onChange('')
                inputRef.current?.focus()
              }}
            >
              <IconCircleXFilled size="1.1em" aria-hidden />
            </button>
          )}
        </div>
        {cancelable && active && (
          <button type="button" className={classes.cancel} onClick={cancel}>
            Cancel
          </button>
        )}
      </div>
    </search>
  )
}
