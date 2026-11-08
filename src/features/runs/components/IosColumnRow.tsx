import { ActionIcon, Menu } from '@mantine/core'
import { IconCheck, IconMenu2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import classes from '../../../components/common/IosPipelineList.module.css'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import { type ColumnItem, canMoveColumn } from '../lib/columnCustomization'

interface IosColumnRowProps {
  item: ColumnItem
  items: ColumnItem[]
  onToggle: (id: string) => void
  onMove: (id: string, direction: -1 | 1) => void
}

function ReorderMenu({ item, items, onMove }: Omit<IosColumnRowProps, 'onToggle'>): ReactNode {
  return (
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
  )
}

export function IosColumnRow({ item, items, onToggle, onMove }: IosColumnRowProps): ReactNode {
  return (
    <div className={classes.row} data-flat="" data-trailing={item.moveable ? '' : undefined}>
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
      {item.moveable && <ReorderMenu item={item} items={items} onMove={onMove} />}
    </div>
  )
}
