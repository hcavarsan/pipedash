import { ActionIcon, Box, Button, Checkbox, Group, ScrollArea, Stack, Text, TextInput, Tooltip } from '@mantine/core'
import {
  IconChevronDown,
  IconChevronUp,
  IconEye,
  IconEyeOff,
  IconRefresh,
  IconSearch,
  IconX,
} from '@tabler/icons-react'
import type { DataTableColumn } from 'mantine-datatable'
import { useEffect, useState } from 'react'

import { TYPOGRAPHY } from '../../theme/typography'
import type { PipelineRun } from '../../types'
import { EmptyState } from '../common/EmptyState'
import { StandardModal } from '../common/StandardModal'

interface TableCustomizationModalProps {
  opened: boolean
  onClose: () => void
  columns: DataTableColumn<PipelineRun>[]
  visibleColumns: DataTableColumn<PipelineRun>[]
  onApply: (columnOrder: string[], columnVisibility: Record<string, boolean>) => void
  currentOrder?: string[]
  currentVisibility?: Record<string, boolean>
}

interface ColumnItem {
  id: string
  title: string
  visible: boolean
  locked: boolean
  moveable: boolean
}

export function TableCustomizationModal({
  opened,
  onClose,
  columns,
  visibleColumns,
  onApply,
  currentOrder,
  currentVisibility,
}: TableCustomizationModalProps) {
  const [columnItems, setColumnItems] = useState<ColumnItem[]>([])
  const [searchQuery, setSearchQuery] = useState('')

  useEffect(() => {
    if (!opened) {
      setSearchQuery('')

      return
    }

    const currentlyVisibleIds = new Set(visibleColumns.map((col) => String(col.accessor)))

    const items: ColumnItem[] = columns.map((col) => {
      const id = String(col.accessor)
      const isLocked = id === 'status' || id === 'actions'
      const isMoveable = id !== 'actions'

      return {
        id,
        title: String(col.title || id),
        visible: currentVisibility?.[id] ?? currentlyVisibleIds.has(id),
        locked: isLocked,
        moveable: isMoveable,
      }
    })

    if (currentOrder && currentOrder.length > 0) {
      const orderedItems: ColumnItem[] = []
      const itemMap = new Map(items.map((item) => [item.id, item]))

      for (const id of currentOrder) {
        const item = itemMap.get(id)

        if (item) {
          orderedItems.push(item)
          itemMap.delete(id)
        }
      }

      orderedItems.push(...itemMap.values())
      setColumnItems(orderedItems)
    } else {
      setColumnItems(items)
    }
  }, [opened, visibleColumns, columns, currentOrder, currentVisibility])

  const handleToggleVisibility = (id: string) => {
    setColumnItems((prev) => prev.map((item) => (item.id === id ? { ...item, visible: !item.visible } : item)))
  }

  const handleToggleAll = () => {
    const allVisible = columnItems.every((item) => item.visible || item.locked)

    setColumnItems((prev) => prev.map((item) => (item.locked ? item : { ...item, visible: !allVisible })))
  }

  const handleMoveUp = (index: number) => {
    if (index === 0 || !columnItems[index].moveable) {
      return
    }

    const newItems = [...columnItems]
    const [item] = newItems.splice(index, 1)

    newItems.splice(index - 1, 0, item)
    setColumnItems(newItems)
  }

  const handleMoveDown = (index: number) => {
    if (index === columnItems.length - 1 || !columnItems[index].moveable) {
      return
    }

    const newItems = [...columnItems]
    const [item] = newItems.splice(index, 1)

    newItems.splice(index + 1, 0, item)
    setColumnItems(newItems)
  }

  const handleApply = () => {
    const order = columnItems.map((item) => item.id)
    const visibility = columnItems.reduce(
      (acc, item) => {
        acc[item.id] = item.visible

        return acc
      },
      {} as Record<string, boolean>
    )

    onApply(order, visibility)
    onClose()
  }

  const handleReset = () => {
    const defaultItems: ColumnItem[] = columns.map((col) => {
      const id = String(col.accessor)

      return {
        id,
        title: String(col.title || id),
        visible: true,
        locked: id === 'status' || id === 'actions',
        moveable: id !== 'actions',
      }
    })

    setColumnItems(defaultItems)
  }

  const filteredItems = columnItems.filter((item) => item.title.toLowerCase().includes(searchQuery.toLowerCase()))

  const visibleCount = columnItems.filter((item) => item.visible).length
  const totalCount = columnItems.length
  const allVisible = columnItems.every((item) => item.visible || item.locked)
  const someVisible = columnItems.some((item) => item.visible && !item.locked)

  const modalTitle = (
    <Group gap="xs">
      <Text fw={TYPOGRAPHY.MODAL_TITLE.weight} size={TYPOGRAPHY.MODAL_TITLE.size}>
        Customize columns
      </Text>
      <Text size={TYPOGRAPHY.HELPER_TEXT.size} c="dimmed">
        {visibleCount}/{totalCount}
      </Text>
    </Group>
  )

  const modalFooter = (
    <Group justify="space-between" gap="sm" wrap="nowrap">
      <Button variant="subtle" color="gray" size="sm" leftSection={<IconRefresh size={16} />} onClick={handleReset}>
        Reset
      </Button>

      <Group gap="sm" wrap="nowrap">
        <Button variant="default" size="sm" onClick={onClose}>
          Cancel
        </Button>
        <Button size="sm" onClick={handleApply}>
          Apply
        </Button>
      </Group>
    </Group>
  )

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={modalTitle}
      footer={modalFooter}
      size="lg"
      disableScrollArea
      contentPadding={false}
    >
      <Box px="lg" py="md">
        <TextInput
          aria-label="Search columns"
          placeholder="Search columns..."
          leftSection={<IconSearch size={16} />}
          rightSection={
            searchQuery && (
              <ActionIcon
                size="sm"
                variant="subtle"
                color="gray"
                aria-label="Clear search"
                onClick={() => setSearchQuery('')}
              >
                <IconX size={14} />
              </ActionIcon>
            )
          }
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
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
            onChange={handleToggleAll}
            size="xs"
          />
          <Group gap="xs">
            <Tooltip label="Show all columns">
              <ActionIcon
                size="sm"
                variant="subtle"
                color="gray"
                aria-label="Show all columns"
                onClick={() => setColumnItems((prev) => prev.map((item) => ({ ...item, visible: true })))}
              >
                <IconEye size={14} />
              </ActionIcon>
            </Tooltip>
            <Tooltip label="Hide optional columns">
              <ActionIcon
                size="sm"
                variant="subtle"
                color="gray"
                aria-label="Hide optional columns"
                onClick={() =>
                  setColumnItems((prev) => prev.map((item) => (item.locked ? item : { ...item, visible: false })))
                }
              >
                <IconEyeOff size={14} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Group>
      </Box>

      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap={0}>
          {filteredItems.length === 0 ? (
            <EmptyState
              title={columnItems.length === 0 ? 'No columns available' : 'No columns match your search'}
              description={
                columnItems.length === 0
                  ? "This provider's table schema has no columns to customize."
                  : `No column title contains "${searchQuery}".`
              }
              action={
                columnItems.length > 0 && (
                  <Button variant="default" size="sm" onClick={() => setSearchQuery('')}>
                    Clear search
                  </Button>
                )
              }
            />
          ) : (
            filteredItems.map((item) => {
              const actualIndex = columnItems.indexOf(item)

              return (
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
                  <Group gap={2} wrap="nowrap" style={{ visibility: item.moveable ? 'visible' : 'hidden' }}>
                    <ActionIcon
                      size="xs"
                      variant="subtle"
                      color="gray"
                      aria-label={`Move ${item.title} up`}
                      onClick={() => handleMoveUp(actualIndex)}
                      disabled={!item.moveable || actualIndex === 0}
                    >
                      <IconChevronUp size={14} />
                    </ActionIcon>
                    <ActionIcon
                      size="xs"
                      variant="subtle"
                      color="gray"
                      aria-label={`Move ${item.title} down`}
                      onClick={() => handleMoveDown(actualIndex)}
                      disabled={!item.moveable || actualIndex === columnItems.length - 1}
                    >
                      <IconChevronDown size={14} />
                    </ActionIcon>
                  </Group>

                  <Checkbox
                    flex={1}
                    size="xs"
                    label={item.title}
                    checked={item.visible}
                    onChange={() => handleToggleVisibility(item.id)}
                    disabled={item.locked}
                  />

                  {item.locked && (
                    <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed" fs="italic">
                      Required
                    </Text>
                  )}
                </Group>
              )
            })
          )}
        </Stack>
      </ScrollArea>
    </StandardModal>
  )
}
