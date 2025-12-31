import { useEffect, useState } from 'react'
import type { DataTableColumn } from 'mantine-datatable'

import {
  ActionIcon,
  Box,
  Button,
  Checkbox,
  Group,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  UnstyledButton,
} from '@mantine/core'
import {
  IconChevronDown,
  IconChevronUp,
  IconEye,
  IconEyeOff,
  IconRefresh,
  IconSearch,
  IconX,
} from '@tabler/icons-react'

import type { PipelineRun } from '../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { StandardModal } from '../common/StandardModal'

interface TableCustomizationModalProps {
  opened: boolean;
  onClose: () => void;
  columns: DataTableColumn<PipelineRun>[];
  visibleColumns: DataTableColumn<PipelineRun>[];
  onApply: (columnOrder: string[], columnVisibility: Record<string, boolean>) => void;
  currentOrder?: string[];
  currentVisibility?: Record<string, boolean>;
}

interface ColumnItem {
  id: string;
  title: string;
  visible: boolean;
  locked: boolean;
  moveable: boolean;
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

    const currentlyVisibleIds = new Set(visibleColumns.map(col => String(col.accessor)))

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
      const itemMap = new Map(items.map(item => [item.id, item]))

      for (const id of currentOrder) {
        const item = itemMap.get(id)


        if (item) {
          orderedItems.push(item)
          itemMap.delete(id)
        }
      }

      itemMap.forEach(item => orderedItems.push(item))
      setColumnItems(orderedItems)
    } else {
      setColumnItems(items)
    }
  }, [opened, visibleColumns, columns, currentOrder, currentVisibility])

  const handleToggleVisibility = (id: string) => {
    setColumnItems(prev =>
      prev.map(item =>
        item.id === id ? { ...item, visible: !item.visible } : item
      )
    )
  }

  const handleToggleAll = () => {
    const allVisible = columnItems.every(item => item.visible || item.locked)


    setColumnItems(prev =>
      prev.map(item =>
        item.locked ? item : { ...item, visible: !allVisible }
      )
    )
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
    const order = columnItems.map(item => item.id)
    const visibility = columnItems.reduce((acc, item) => {
      acc[item.id] = item.visible

      return acc
    }, {} as Record<string, boolean>)

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

  const filteredItems = columnItems.filter(item =>
    item.title.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const visibleCount = columnItems.filter(item => item.visible).length
  const totalCount = columnItems.length
  const allVisible = columnItems.every(item => item.visible || item.locked)
  const someVisible = columnItems.some(item => item.visible && !item.locked)

  const modalTitle = (
    <Group gap="xs">
      <Text fw={THEME_TYPOGRAPHY.MODAL_TITLE.weight} size={THEME_TYPOGRAPHY.MODAL_TITLE.size} c={THEME_COLORS.TITLE}>
        Customize Columns
      </Text>
      <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>
        {visibleCount}/{totalCount}
      </Text>
    </Group>
  )

  const modalFooter = (
    <Group justify="space-between" gap={0} wrap="nowrap">
      <Button
        variant="subtle"
        leftSection={<IconRefresh size={16} />}
        onClick={handleReset}
        size="sm"
        color="gray"
      >
        Reset
      </Button>

      <Group gap="xs" wrap="nowrap">
        <Button
          variant="default"
          onClick={onClose}
          size="sm"
          color="dark"
        >
          Cancel
        </Button>
        <Button
          onClick={handleApply}
          size="sm"
          variant="filled"
        >
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
      disableScrollArea
      contentPadding={false}
    >
      <Box px="lg" pb="md" pt="md">
        <TextInput
          placeholder="Search columns..."
          leftSection={<IconSearch size={16} />}
          rightSection={
            searchQuery && (
              <ActionIcon
                size="sm"
                variant="subtle"
                color="gray"
                onClick={() => setSearchQuery('')}
              >
                <IconX size={14} />
              </ActionIcon>
            )
          }
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          size="sm"
          radius="md"
          styles={{
            input: {
              backgroundColor: 'var(--mantine-color-dark-7)',
              border: '1px solid var(--mantine-color-dark-5)',
              color: 'var(--mantine-color-gray-1)',
            },
          }}
        />
      </Box>

      <Box px="lg" py="xs" style={{ borderTop: '1px solid var(--mantine-color-dark-6)', borderBottom: '1px solid var(--mantine-color-dark-6)' }}>
        <Group justify="space-between">
          <Checkbox
            label={
              <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>
                {allVisible ? 'Deselect all' : 'Select all'}
              </Text>
            }
            checked={allVisible}
            indeterminate={!allVisible && someVisible}
            onChange={handleToggleAll}
            size="xs"
          />
          <Group gap={4}>
            <ActionIcon
              size="sm"
              variant="subtle"
              color="gray"
              onClick={() => setColumnItems(prev =>
                prev.map(item => ({ ...item, visible: true }))
              )}
            >
              <IconEye size={14} />
            </ActionIcon>
            <ActionIcon
              size="sm"
              variant="subtle"
              color="gray"
              onClick={() => setColumnItems(prev =>
                prev.map(item => item.locked ? item : { ...item, visible: false })
              )}
            >
              <IconEyeOff size={14} />
            </ActionIcon>
          </Group>
        </Group>
      </Box>

      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap={0}>
          {filteredItems.length === 0 ? (
            <Box p="xl" style={{ textAlign: 'center' }}>
              <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>
                No columns found
              </Text>
            </Box>
          ) : (
            filteredItems.map((item) => {
              const actualIndex = columnItems.indexOf(item)



              return (
                <UnstyledButton
                  key={item.id}
                  onClick={() => !item.locked && handleToggleVisibility(item.id)}
                  style={{
                    borderBottom: '1px solid var(--mantine-color-dark-6)',
                    transition: 'all 150ms ease',
                    backgroundColor: item.visible
                      ? 'rgba(34, 139, 230, 0.04)'
                      : 'transparent',
                    borderLeft: item.visible
                      ? '3px solid rgba(34, 139, 230, 0.5)'
                      : '3px solid transparent',
                  }}
                  styles={{
                    root: {
                      '&:hover': {
                        backgroundColor: item.visible
                          ? 'rgba(34, 139, 230, 0.08)'
                          : 'var(--mantine-color-dark-7)',
                      },
                    },
                  }}
                >
                  <Group px="lg" py="sm" gap="sm" wrap="nowrap">
                    <Group gap={2} wrap="nowrap">
                      <ActionIcon
                        component="div"
                        size="xs"
                        variant="subtle"
                        color="gray"
                        onClick={(e) => {
                          e.stopPropagation()
                          handleMoveUp(actualIndex)
                        }}
                        disabled={!item.moveable || actualIndex === 0}
                        style={{
                          visibility: item.moveable ? 'visible' : 'hidden',
                          opacity: !item.moveable || actualIndex === 0 ? 0.2 : 0.6,
                          cursor: !item.moveable || actualIndex === 0 ? 'default' : 'pointer',
                        }}
                      >
                        <IconChevronUp size={14} />
                      </ActionIcon>
                      <ActionIcon
                        component="div"
                        size="xs"
                        variant="subtle"
                        color="gray"
                        onClick={(e) => {
                          e.stopPropagation()
                          handleMoveDown(actualIndex)
                        }}
                        disabled={!item.moveable || actualIndex === columnItems.length - 1}
                        style={{
                          visibility: item.moveable ? 'visible' : 'hidden',
                          opacity: !item.moveable || actualIndex === columnItems.length - 1 ? 0.2 : 0.6,
                          cursor: !item.moveable || actualIndex === columnItems.length - 1 ? 'default' : 'pointer',
                        }}
                      >
                        <IconChevronDown size={14} />
                      </ActionIcon>
                    </Group>

                    <Checkbox
                      checked={item.visible}
                      onChange={() => handleToggleVisibility(item.id)}
                      disabled={item.locked}
                      onClick={(e) => e.stopPropagation()}
                      size="xs"
                      style={{ pointerEvents: 'auto' }}
                    />

                    <Text
                      size="sm"
                      fw={item.visible ? 500 : 400}
                      c={item.visible ? 'gray.1' : 'gray.6'}
                      style={{ flex: 1 }}
                    >
                      {item.title}
                    </Text>

                    {item.locked && (
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.DIMMED} fs="italic">
                        Required
                      </Text>
                    )}
                  </Group>
                </UnstyledButton>
              )
            })
          )}
        </Stack>
      </ScrollArea>
    </StandardModal>
  )
}
