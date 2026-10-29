import { ActionIcon, Box, Button, Checkbox, Group, ScrollArea, Stack, Text, TextInput, Tooltip } from '@mantine/core'
import { IconChevronDown, IconChevronUp, IconEye, IconEyeOff, IconSearch, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { TYPOGRAPHY } from '../../../theme/typography'

import { canMoveColumn } from '../lib/columnCustomization'
import type { ColumnCustomizationBodyProps } from './IosColumnCustomization'

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
  const allVisible = items.every((item) => item.visible || item.locked)
  const someVisible = items.some((item) => item.visible && !item.locked)

  return (
    <>
      <Box px="lg" py="md">
        <TextInput
          aria-label="Search columns"
          placeholder="Search columns..."
          leftSection={<IconSearch size={16} aria-hidden />}
          rightSection={
            search && (
              <Tooltip label="Clear search">
                <ActionIcon type="button" size="sm" aria-label="Clear search" onClick={() => onSearchChange('')}>
                  <IconX size={14} aria-hidden />
                </ActionIcon>
              </Tooltip>
            )
          }
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
        <Group justify="space-between">
          <Checkbox
            label={allVisible ? 'Deselect all' : 'Select all'}
            checked={allVisible}
            indeterminate={!allVisible && someVisible}
            onChange={() => (allVisible ? onHideOptional() : onShowAll())}
            size="xs"
          />
          <Group gap="xs">
            <Tooltip label="Show all columns">
              <ActionIcon type="button" size="sm" aria-label="Show all columns" onClick={onShowAll}>
                <IconEye size={14} aria-hidden />
              </ActionIcon>
            </Tooltip>
            <Tooltip label="Hide optional columns">
              <ActionIcon type="button" size="sm" aria-label="Hide optional columns" onClick={onHideOptional}>
                <IconEyeOff size={14} aria-hidden />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Group>
      </Box>

      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap={0}>
          {filtered.length === 0 ? (
            <EmptyState
              title={items.length === 0 ? 'No columns available' : 'No columns match your search'}
              description={
                items.length === 0
                  ? "This provider's table schema has no columns to customize."
                  : `No column title contains "${search}".`
              }
              action={
                items.length > 0 && (
                  <Button type="button" variant="default" size="sm" onClick={() => onSearchChange('')}>
                    Clear search
                  </Button>
                )
              }
            />
          ) : (
            filtered.map((item) => (
              <Group
                key={item.id}
                px="lg"
                py="sm"
                gap="sm"
                wrap="nowrap"
                style={{
                  borderBottom: '1px solid var(--mantine-color-default-border)',
                  borderLeft: `3px solid ${item.visible ? 'var(--mantine-color-blue-light-color)' : 'transparent'}`,
                  backgroundColor: item.visible ? 'var(--mantine-color-blue-light)' : undefined,
                }}
              >
                <Group gap="4xs" wrap="nowrap" style={{ visibility: item.moveable ? 'visible' : 'hidden' }}>
                  <Tooltip label={`Move ${item.title} up`}>
                    <ActionIcon
                      type="button"
                      size="xs"
                      aria-label={`Move ${item.title} up`}
                      onClick={() => onMove(item.id, -1)}
                      disabled={!canMoveColumn(items, item.id, -1)}
                    >
                      <IconChevronUp size={14} aria-hidden />
                    </ActionIcon>
                  </Tooltip>
                  <Tooltip label={`Move ${item.title} down`}>
                    <ActionIcon
                      type="button"
                      size="xs"
                      aria-label={`Move ${item.title} down`}
                      onClick={() => onMove(item.id, 1)}
                      disabled={!canMoveColumn(items, item.id, 1)}
                    >
                      <IconChevronDown size={14} aria-hidden />
                    </ActionIcon>
                  </Tooltip>
                </Group>

                <Checkbox
                  flex={1}
                  size="xs"
                  label={item.title}
                  checked={item.visible}
                  onChange={() => onToggle(item.id)}
                  disabled={item.locked}
                />

                {item.locked && (
                  <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed" fs="italic">
                    Required
                  </Text>
                )}
              </Group>
            ))
          )}
        </Stack>
      </ScrollArea>
    </>
  )
}
