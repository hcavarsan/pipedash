import { IconCircleXFilled, IconSearch } from '@tabler/icons-react'
import { useRef, useState } from 'react'

import classes from './PhoneSearchField.module.css'

interface PhoneSearchFieldProps {
  value: string
  placeholder?: string
  onChange: (value: string) => void
  onClear: () => void
}

export function PhoneSearchField({ value, placeholder = 'Search', onChange, onClear }: PhoneSearchFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [active, setActive] = useState(false)

  const cancel = () => {
    onClear()
    inputRef.current?.blur()
    setActive(false)
  }

  return (
    <search className={classes.search}>
      <form
        className={classes.form}
        onSubmit={(event) => {
          event.preventDefault()
          inputRef.current?.blur()
        }}
      >
        <div className={classes.field}>
          <IconSearch className={classes.icon} size="1.1em" aria-hidden />
          <input
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
        {active && (
          <button type="button" className={classes.cancel} onClick={cancel}>
            Cancel
          </button>
        )}
      </form>
    </search>
  )
}
