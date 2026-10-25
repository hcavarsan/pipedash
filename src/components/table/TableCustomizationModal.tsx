import {
  ActionIcon,
  Box,
  Button,
  Checkbox,
  Group,
  Menu,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import {
  IconCheck,
  IconChevronDown,
  IconChevronUp,
  IconDots,
  IconEye,
  IconEyeOff,
  IconRefresh,
  IconSearch,
  IconX,
} from '@tabler/icons-react'
import type { DataTableColumn } from 'mantine-datatable'
import { useEffect, useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { TYPOGRAPHY } from '../../theme/typography'
import type { PipelineRun } from '../../types'
import { EmptyState } from '../common/EmptyState'
import { StandardModal } from '../common/StandardModal'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../ios/IosList'

import classes from './IosPipelineList.module.css'

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

const signature = (items: ColumnItem[]): string => items.map((item) => `${item.id}:${item.visible ? 1 : 0}`).join(',')

export function TableCustomizationModal({
  opened,
  onClose,
  columns,
  visibleColumns,
  onApply,
  currentOrder,
  currentVisibility,
}: TableCustomizationModalProps) {
  const mobileShell = useMobileShell()
  const [columnItems, setColumnItems] = useState<ColumnItem[]>([])
  const [initialSignature, setInitialSignature] = useState('')
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
      setInitialSignature(signature(orderedItems))
    } else {
      setColumnItems(items)
      setInitialSignature(signature(items))
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
  const dirty = columnItems.length > 0 && signature(columnItems) !== initialSignature

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

  const modalFooter = (close: () => void) => (
    <Group justify="space-between" gap="sm" wrap="nowrap">
      <Button variant="subtle" color="gray" size="sm" leftSection={<IconRefresh size={16} />} onClick={handleReset}>
        Reset
      </Button>

      <Group gap="sm" wrap="nowrap">
        <Button variant="default" size="sm" onClick={close}>
          Cancel
        </Button>
        <Button size="sm" onClick={handleApply}>
          Apply
        </Button>
      </Group>
    </Group>
  )

  if (mobileShell) {
    return (
      <StandardModal
        opened={opened}
        onClose={onClose}
        title="Customize columns"
        size="md"
        fullHeight
        dirty={dirty}
        primaryAction={{ label: 'Apply', onClick: handleApply }}
      >
        <Stack gap="lg">
          <TextInput
            aria-label="Search columns"
            placeholder="Search columns"
            leftSection={<IconSearch size={18} />}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />

          <IosGroup>
            <IosRow
              title="Show all columns"
              onClick={() => setColumnItems((prev) => prev.map((item) => ({ ...item, visible: true })))}
            />
            <IosRow
              title="Hide optional columns"
              onClick={() =>
                setColumnItems((prev) => prev.map((item) => (item.locked ? item : { ...item, visible: false })))
              }
            />
            <IosRow title="Reset to defaults" onClick={handleReset} />
          </IosGroup>

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
                  <Button variant="default" onClick={() => setSearchQuery('')}>
                    Clear search
                  </Button>
                )
              }
            />
          ) : (
            <IosGroup
              header={`Columns · ${visibleCount} of ${totalCount} shown`}
              footer="Hidden columns are not shown in the table. Required columns cannot be hidden."
            >
              {filteredItems.map((item) => {
                const actualIndex = columnItems.indexOf(item)

                return (
                  <div
                    key={item.id}
                    className={classes.row}
                    data-flat=""
                    data-trailing={item.moveable ? '' : undefined}
                  >
                    <button
                      type="button"
                      className={classes.main}
                      disabled={item.locked}
                      aria-pressed={item.visible}
                      onClick={() => handleToggleVisibility(item.id)}
                    >
                      <span className={classes.text}>
                        <span className={classes.title}>{item.title}</span>
                      </span>
                      {item.locked && <span className={classes.value}>Required</span>}
                      <span className={classes.check} aria-hidden>
                        {item.visible && <IconCheck size={20} />}
                      </span>
                    </button>
                    {item.moveable && (
                      <div className={classes.trailing}>
                        <Menu position="bottom">
                          <Menu.Target>
                            <ActionIcon variant="subtle" color="gray" aria-label={`Reorder ${item.title}`}>
                              <IconDots size={22} />
                            </ActionIcon>
                          </Menu.Target>
                          <Menu.Dropdown>
                            <Menu.Label>{item.title}</Menu.Label>
                            <Menu.Item disabled={actualIndex === 0} onClick={() => handleMoveUp(actualIndex)}>
                              Move up
                            </Menu.Item>
                            <Menu.Item
                              disabled={actualIndex === columnItems.length - 1}
                              onClick={() => handleMoveDown(actualIndex)}
                            >
                              Move down
                            </Menu.Item>
                            <ActionSheetCancel />
                          </Menu.Dropdown>
                        </Menu>
                      </div>
                    )}
                  </div>
                )
              })}
            </IosGroup>
          )}
        </Stack>
      </StandardModal>
    )
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={modalTitle}
      footer={modalFooter}
      dirty={dirty}
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
              <Tooltip label="Clear search">
                <ActionIcon
                  size="sm"
                  variant="subtle"
                  color="gray"
                  aria-label="Clear search"
                  onClick={() => setSearchQuery('')}
                >
                  <IconX size={14} />
                </ActionIcon>
              </Tooltip>
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
                  <Group gap="4xs" wrap="nowrap" style={{ visibility: item.moveable ? 'visible' : 'hidden' }}>
                    <Tooltip label={`Move ${item.title} up`}>
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
                    </Tooltip>
                    <Tooltip label={`Move ${item.title} down`}>
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
                    </Tooltip>
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
