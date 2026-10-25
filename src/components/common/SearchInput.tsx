import { ActionIcon, Group, Loader, type MantineSize, TextInput, Tooltip } from '@mantine/core'
import { IconSearch, IconX } from '@tabler/icons-react'
import { type CSSProperties, type Ref, useRef } from 'react'

interface SearchInputProps {
  value: string
  onChange: (value: string) => void
  placeholder: string
  'aria-label'?: string
  loading?: boolean
  loadingLabel?: string
  autoFocus?: boolean
  inputRef?: Ref<HTMLInputElement>
  size?: MantineSize
  disabled?: boolean
  style?: CSSProperties
  className?: string
}

export function SearchInput({
  value,
  onChange,
  placeholder,
  'aria-label': ariaLabel,
  loading = false,
  loadingLabel = 'Searching…',
  autoFocus,
  inputRef,
  size,
  disabled,
  style,
  className,
}: SearchInputProps) {
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
      size={size}
      className={className}
      leftSection={<IconSearch size={16} aria-hidden />}
      rightSectionWidth={loading && value ? 64 : undefined}
      disabled={disabled}
      style={style}
      rightSection={
        (loading || value) && (
          <Group gap="3xs" wrap="nowrap">
            {loading && <Loader size="xs" role="status" aria-label={loadingLabel} />}
            {value && (
              <Tooltip label="Clear search">
                <ActionIcon
                  type="button"
                  variant="subtle"
                  color="gray"
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
