import { ActionIcon, Box, Button, Group, Stack, TextInput, Tooltip } from '@mantine/core'
import { IconSearch, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { FillScrollArea } from '../../../components/common/FillScrollArea'
import type { ColumnItem } from '../lib/columnCustomization'
import { ColumnBulkActions } from './ColumnBulkActions'
import { DesktopColumnRow } from './DesktopColumnRow'

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

export function DesktopColumnCustomization({
  items,
  search,
  onSearchChange,
  onToggle,
  onMove,
  onShowAll,
  onHideOptional,
}: ColumnCustomizationBodyProps): ReactNode {
  const filtered = items.filter((item) => item.title.toLowerCase().includes(search.toLowerCase()))
  const isEmpty = items.length === 0
  const emptyTitle = isEmpty ? 'No columns available' : 'No columns match your search'
  const emptyDescription = isEmpty
    ? "This provider's table schema has no columns to customize."
    : `No column title contains "${search}".`
  const emptyAction = isEmpty ? null : (
    <Button type="button" variant="default" size="sm" onClick={() => onSearchChange('')}>
      Clear search
    </Button>
  )
  const clearSearchButton = search ? (
    <Tooltip label="Clear search">
      <ActionIcon type="button" size="sm" aria-label="Clear search" onClick={() => onSearchChange('')}>
        <IconX size={14} aria-hidden />
      </ActionIcon>
    </Tooltip>
  ) : null

  return (
    <>
      <Box px="lg" py="md">
        <TextInput
          aria-label="Search columns"
          placeholder="Search columns…"
          leftSection={<IconSearch size={16} aria-hidden />}
          rightSection={clearSearchButton}
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
            }
          }}
        />
      </Box>

      <Box
        px="lg"
        py="xs"
        style={{
          borderTop: '1px solid var(--mantine-color-default-border)',
          borderBottom: '1px solid var(--mantine-color-default-border)',
        }}
      >
        <Group justify="flex-end">
          <ColumnBulkActions onShowAll={onShowAll} onHideOptional={onHideOptional} />
        </Group>
      </Box>

      <FillScrollArea type="auto">
        <Stack gap={0}>
          {filtered.length === 0 ? (
            <EmptyState title={emptyTitle} description={emptyDescription} action={emptyAction} />
          ) : (
            filtered.map((item) => (
              <DesktopColumnRow key={item.id} item={item} items={items} onToggle={onToggle} onMove={onMove} />
            ))
          )}
        </Stack>
      </FillScrollArea>
    </>
  )
}
