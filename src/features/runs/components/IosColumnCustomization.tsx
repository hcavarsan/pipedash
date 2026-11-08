import { Button, Stack, TextInput } from '@mantine/core'
import { IconSearch } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { EmptyState } from '../../../components/common/EmptyState'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import type { ColumnItem } from '../lib/columnCustomization'
import { IosColumnRow } from './IosColumnRow'

export interface ColumnCustomizationBodyProps {
  items: ColumnItem[]
  search: string
  onSearchChange: (value: string) => void
  onToggle: (id: string) => void
  onMove: (id: string, direction: -1 | 1) => void
  onShowAll: () => void
  onHideOptional: () => void
  onReset: () => void
}

export function IosColumnCustomization({
  items,
  search,
  onSearchChange,
  onToggle,
  onMove,
  onShowAll,
  onHideOptional,
  onReset,
}: ColumnCustomizationBodyProps): ReactNode {
  const filtered = items.filter((item) => item.title.toLowerCase().includes(search.toLowerCase()))
  const visibleCount = items.filter((item) => item.visible).length
  const isEmpty = items.length === 0
  const emptyTitle = isEmpty ? 'No columns available' : 'No columns match your search'
  const emptyDescription = isEmpty
    ? "This provider's table schema has no columns to customize."
    : `No column title contains "${search}".`
  const emptyAction = isEmpty ? null : (
    <Button type="button" variant="default" onClick={() => onSearchChange('')}>
      Clear search
    </Button>
  )

  return (
    <Stack gap="lg">
      <TextInput
        aria-label="Search columns"
        placeholder="Search columns"
        leftSection={<IconSearch size={18} aria-hidden />}
        value={search}
        onChange={(event) => onSearchChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
          }
        }}
      />

      <IosGroup>
        <IosRow title="Show all columns" onClick={onShowAll} />
        <IosRow title="Hide optional columns" onClick={onHideOptional} />
        <IosRow title="Reset to defaults" onClick={onReset} />
      </IosGroup>

      {filtered.length === 0 ? (
        <EmptyState title={emptyTitle} description={emptyDescription} action={emptyAction} />
      ) : (
        <IosGroup
          header={`Columns · ${visibleCount} of ${items.length} shown`}
          footer="Hidden columns are not shown in the table. Required columns cannot be hidden."
        >
          {filtered.map((item) => (
            <IosColumnRow key={item.id} item={item} items={items} onToggle={onToggle} onMove={onMove} />
          ))}
        </IosGroup>
      )}
    </Stack>
  )
}
