import { ActionIcon, Group, Loader, type MantineSize, TextInput, Tooltip } from '@mantine/core'
import { IconSearch, IconX } from '@tabler/icons-react'
import { type CSSProperties, type ReactNode, type Ref, useRef } from 'react'

interface SearchInputProps {
  value: string
  onChange: (value: string) => void
  placeholder: string
  'aria-label'?: string | undefined
  loading?: boolean | undefined
  loadingLabel?: string | undefined
  autoFocus?: boolean | undefined
  inputRef?: Ref<HTMLInputElement> | undefined
  size?: MantineSize | undefined
  disabled?: boolean | undefined
  style?: CSSProperties | undefined
  className?: string | undefined
}

export function SearchInput({
  value,
  onChange,
  placeholder,
  'aria-label': ariaLabel,
  loading = false,
  loadingLabel = 'Searching…',
  autoFocus = false,
  inputRef,
  size,
  disabled,
  style,
  className,
}: SearchInputProps): ReactNode {
  const localRef = useRef<HTMLInputElement | null>(null)

  return (
    <TextInput
      ref={(input) => {
        localRef.current = input
        if (typeof inputRef === 'function') {
          return inputRef(input)
        }
        if (inputRef) {
          inputRef.current = input
        }
      }}
      data-search-input
      value={value}
      onChange={(event) => onChange(event.currentTarget.value)}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && value) {
          event.preventDefault()
          event.stopPropagation()
          onChange('')
        }
      }}
      placeholder={placeholder}
      aria-label={ariaLabel ?? placeholder}
      autoFocus={autoFocus}
      leftSection={<IconSearch size={16} aria-hidden />}
      disabled={disabled ?? false}
      {...(size === undefined ? {} : { size })}
      {...(className === undefined ? {} : { className })}
      {...(style === undefined ? {} : { style })}
      {...(loading && value ? { rightSectionWidth: 64 } : {})}
      rightSection={
        (loading || value) && (
          <Group gap="3xs" wrap="nowrap">
            {loading && <Loader size="xs" role="status" aria-label={loadingLabel} />}
            {value && (
              <Tooltip label="Clear search">
                <ActionIcon
                  type="button"
                  size="sm"
                  aria-label="Clear search"
                  onClick={() => {
                    onChange('')
                    localRef.current?.focus()
                  }}
                >
                  <IconX size={14} aria-hidden />
                </ActionIcon>
              </Tooltip>
            )}
          </Group>
        )
      }
    />
  )
}
