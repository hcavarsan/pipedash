import { MultiSelect, Select, Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import { IosGroup, IosRow } from '../ios/IosList'

import type { FilterSheetProps } from './filterTypes'
import { StandardModal } from './StandardModal'

export const FilterSheet = ({
  opened,
  onClose,
  status,
  statusOptions,
  selects,
  hasActiveFilters,
  onClear,
}: FilterSheetProps): ReactNode => {
  const statusRow = status && (
    <IosRow
      title="Status"
      accessory={
        <MultiSelect
          aria-label="Status"
          placeholder={status.value.length > 0 ? undefined : 'All'}
          data={statusOptions}
          value={status.value}
          onOptionSubmit={(value) => status.onSelect(value, true)}
          onRemove={(value) => status.onSelect(value, false)}
          onClear={status.onClear}
          variant="unstyled"
          w={190}
          styles={{
            input: { background: 'transparent', paddingLeft: 0 },
            pillsList: { justifyContent: 'flex-end' },
          }}
        />
      }
    />
  )

  const selectRows = selects.map(({ key, label, data, value, onChange, searchable }) => (
    <IosRow
      key={key}
      title={label}
      accessory={
        <Select
          aria-label={label}
          placeholder="All"
          data={data}
          value={value}
          onChange={onChange}
          searchable={searchable}
          variant="unstyled"
          w={190}
          styles={{ input: { background: 'transparent', textAlign: 'right', paddingLeft: 0 } }}
        />
      }
    />
  ))

  const clearAndClose = (): void => {
    onClear()
    onClose()
  }

  const clearGroup = hasActiveFilters && (
    <IosGroup>
      <IosRow title="Clear filters" destructive onClick={clearAndClose} />
    </IosGroup>
  )

  return (
    <StandardModal opened={opened} onClose={onClose} title="Filters" size="md">
      <Stack gap="lg">
        <IosGroup>
          {statusRow}
          {selectRows}
        </IosGroup>
        {clearGroup}
      </Stack>
    </StandardModal>
  )
}
