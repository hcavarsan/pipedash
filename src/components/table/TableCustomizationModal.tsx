import { useEffect, useState } from 'react'
import type { DataTableColumn } from 'mantine-datatable'

import { ActionIcon, Button, Group, Modal, Paper, Stack, Switch, Text } from '@mantine/core'
import { IconChevronDown, IconChevronUp, IconLock } from '@tabler/icons-react'

import type { PipelineRun } from '../../types'

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

  useEffect(() => {
    if (!opened) {
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

      // Add items in current order
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, visibleColumns])

  const handleToggleVisibility = (id: string) => {
    setColumnItems(prev =>
      prev.map(item =>
        item.id === id ? { ...item, visible: !item.visible } : item
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

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Customize Table Columns"
      size="md"
      centered
    >
      <Stack gap="xs">
        <Text size="sm" c="dimmed">
          Use arrows to reorder, toggle to show/hide columns
        </Text>

        <Stack gap={4}>
          {columnItems.map((item, index) => (
            <Paper
              key={item.id}
              p="xs"
              withBorder
            >
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Group gap={4} wrap="nowrap">
                  <Group gap={2} wrap="nowrap">
                    <ActionIcon
                      size="sm"
                      variant="subtle"
                      color="gray"
                      onClick={() => handleMoveUp(index)}
                      disabled={!item.moveable || index === 0}
                      style={{ visibility: item.moveable ? 'visible' : 'hidden' }}
                    >
                      <IconChevronUp size={16} />
                    </ActionIcon>
                    <ActionIcon
                      size="sm"
                      variant="subtle"
                      color="gray"
                      onClick={() => handleMoveDown(index)}
                      disabled={!item.moveable || index === columnItems.length - 1}
                      style={{ visibility: item.moveable ? 'visible' : 'hidden' }}
                    >
                      <IconChevronDown size={16} />
                    </ActionIcon>
                  </Group>

                  <Text size="sm" fw={500} c={item.visible ? undefined : 'dimmed'}>
                    {item.title}
                  </Text>

                  {!item.visible && !item.locked && (
                    <Text size="xs" c="dimmed">(hidden)</Text>
                  )}

                  {item.locked && (
                    <IconLock size={14} color="var(--mantine-color-gray-6)" />
                  )}
                </Group>

                <Switch
                  checked={item.visible}
                  onChange={() => handleToggleVisibility(item.id)}
                  disabled={item.locked}
                  size="sm"
                />
              </Group>
            </Paper>
          ))}
        </Stack>

        <Group justify="space-between" mt="md">
          <Button variant="subtle" onClick={handleReset} size="sm">
            Reset to Default
          </Button>

          <Group gap="xs">
            <Button variant="default" onClick={onClose} size="sm">
              Cancel
            </Button>
            <Button onClick={handleApply} size="sm">
              Apply
            </Button>
          </Group>
        </Group>
      </Stack>
    </Modal>
  )
}
