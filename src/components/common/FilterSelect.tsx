import { ActionIcon, Select, Tooltip } from '@mantine/core'
import { IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import type { FilterSelectProps } from './filterTypes'

export const FilterSelect = ({
  label,
  data,
  value,
  onChange,
  searchable,
  leftSection,
  clearable = true,
}: FilterSelectProps): ReactNode => (
  <Select
    aria-label={label}
    placeholder={label}
    data={data}
    value={value}
    onChange={onChange}
    searchable={searchable ?? false}
    rightSection={
      value && clearable ? (
        <Tooltip label={`Clear ${label.toLowerCase()} filter`}>
          <ActionIcon
            size="xs"
            aria-label={`Clear ${label.toLowerCase()} filter`}
            onMouseDown={(event) => event.preventDefault()}
            onClick={(event) => {
              event.stopPropagation()
              onChange(null)
            }}
          >
            <IconX size={14} aria-hidden />
          </ActionIcon>
        </Tooltip>
      ) : null
    }
    leftSection={leftSection}
    {...(value && clearable ? { rightSectionPointerEvents: 'all' as const } : {})}
    miw={130}
    flex={1}
    maw="100%"
  />
)
