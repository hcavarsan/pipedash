import { ActionIcon, Checkbox, Group, Text, Tooltip } from '@mantine/core'
import { IconChevronDown, IconChevronUp } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { TYPOGRAPHY } from '../../../theme/typography'
import { type ColumnItem, canMoveColumn } from '../lib/columnCustomization'

interface ColumnMoveButtonsProps {
  item: ColumnItem
  items: ColumnItem[]
  onMove: (id: string, direction: -1 | 1) => void
}

function ColumnMoveButtons({ item, items, onMove }: ColumnMoveButtonsProps): ReactNode {
  return (
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
  )
}

interface DesktopColumnRowProps {
  item: ColumnItem
  items: ColumnItem[]
  onToggle: (id: string) => void
  onMove: (id: string, direction: -1 | 1) => void
}

export function DesktopColumnRow({ item, items, onToggle, onMove }: DesktopColumnRowProps): ReactNode {
  return (
    <Group
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
      <ColumnMoveButtons item={item} items={items} onMove={onMove} />

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
  )
}
