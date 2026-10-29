import { ActionIcon, Button, Menu, Stack, TextInput } from '@mantine/core'
import { IconCheck, IconMenu2, IconSearch } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { EmptyState } from '../../../components/common/EmptyState'
import classes from '../../../components/common/IosPipelineList.module.css'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { type ColumnItem, canMoveColumn } from '../lib/columnCustomization'

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
        <EmptyState
          title={items.length === 0 ? 'No columns available' : 'No columns match your search'}
          description={
            items.length === 0
              ? "This provider's table schema has no columns to customize."
              : `No column title contains "${search}".`
          }
          action={
            items.length > 0 && (
              <Button type="button" variant="default" onClick={() => onSearchChange('')}>
                Clear search
              </Button>
            )
          }
        />
      ) : (
        <IosGroup
          header={`Columns · ${visibleCount} of ${items.length} shown`}
          footer="Hidden columns are not shown in the table. Required columns cannot be hidden."
        >
          {filtered.map((item) => (
            <div key={item.id} className={classes.row} data-flat="" data-trailing={item.moveable ? '' : undefined}>
              <button
                type="button"
                className={classes.main}
                disabled={item.locked}
                aria-pressed={item.visible}
                onClick={() => onToggle(item.id)}
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
                      <ActionIcon type="button" aria-label={`Reorder ${item.title}`}>
                        <IconMenu2 size={22} aria-hidden />
                      </ActionIcon>
                    </Menu.Target>
                    <Menu.Dropdown>
                      <Menu.Label>{item.title}</Menu.Label>
                      <Menu.Item disabled={!canMoveColumn(items, item.id, -1)} onClick={() => onMove(item.id, -1)}>
                        Move up
                      </Menu.Item>
                      <Menu.Item disabled={!canMoveColumn(items, item.id, 1)} onClick={() => onMove(item.id, 1)}>
                        Move down
                      </Menu.Item>
                      <ActionSheetCancel />
                    </Menu.Dropdown>
                  </Menu>
                </div>
              )}
            </div>
          ))}
        </IosGroup>
      )}
    </Stack>
  )
}
