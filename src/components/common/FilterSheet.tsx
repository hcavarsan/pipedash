import { MultiSelect, Select, Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import { IosGroup, IosRow } from '../ios/IosList'
import { FieldRow, RowSelect } from '../ios/mobileParts'
import fieldClasses from '../ios/mobileParts.module.css'

import type { FilterSheetProps } from './filterTypes'
import { StandardModal } from './StandardModal'

const FIELD_WIDTH = '9em'

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
    <FieldRow title="Status">
      {(id) => (
        <MultiSelect
          id={id}
          placeholder={status.value.length > 0 ? undefined : 'All'}
          data={statusOptions}
          value={status.value}
          onOptionSubmit={(value) => status.onSelect(value, true)}
          onRemove={(value) => status.onSelect(value, false)}
          onClear={status.onClear}
          variant="unstyled"
          w={FIELD_WIDTH}
          classNames={{ root: fieldClasses.field, input: fieldClasses.fieldInput }}
          styles={{ pillsList: { justifyContent: 'flex-end' } }}
        />
      )}
    </FieldRow>
  )

  const selectRows = selects.map(({ key, label, data, value, onChange, searchable }) => (
    <FieldRow key={key} title={label}>
      {(id) =>
        searchable ? (
          <Select
            id={id}
            placeholder="All"
            data={data}
            value={value}
            onChange={onChange}
            searchable
            variant="unstyled"
            w={FIELD_WIDTH}
            classNames={{ root: fieldClasses.field, input: `${fieldClasses.fieldInput} ${fieldClasses.fieldSelect}` }}
          />
        ) : (
          <RowSelect id={id} data={data} value={value ?? ''} onChange={onChange} />
        )
      }
    </FieldRow>
  ))

  const clearAndClose = (): void => {
    onClear()
    onClose()
  }

  const clearGroup = hasActiveFilters && (
    <IosGroup>
      <IosRow title="Clear filters" onClick={clearAndClose} />
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
