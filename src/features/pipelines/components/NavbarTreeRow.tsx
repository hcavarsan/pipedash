import { ActionIcon, Box, Group, NavLink, Text, ThemeIcon } from '@mantine/core'
import {
  IconChevronRight,
  IconFileSettings,
  IconFolder,
  IconFolderCode,
  IconPinFilled,
  type IconStack2,
} from '@tabler/icons-react'
import type { KeyboardEvent, MouseEvent, ReactNode } from 'react'
import { Link } from 'react-router-dom'

import type { PinKind, ViewKind } from '../../../types/domain'

export const ICON_TILE = { rail: 30, list: 28 } as const

export const VIEW_ICONS: Partial<Record<ViewKind, typeof IconStack2>> = {
  pinned: IconPinFilled,
  custom: IconFolder,
  config: IconFileSettings,
}

export const PIN_ICONS: Record<PinKind, typeof IconStack2> = {
  view: IconFolder,
  repository: IconFolderCode,
}

export type MenuProps = { onContextMenu: (event: MouseEvent) => void; onKeyDown: (event: KeyboardEvent) => void }

export const NavTile = ({ icon: Icon, size }: { icon: typeof IconStack2; size: number }): ReactNode => (
  <ThemeIcon size={size} variant="transparent" color="gray" aria-hidden>
    <Icon size={size * 0.6} aria-hidden />
  </ThemeIcon>
)

interface NavbarTreeRowProps {
  depth: number
  label: string
  icon: ReactNode
  to?: string | undefined
  active: boolean
  description?: ReactNode
  expandable?: boolean | undefined
  open?: boolean | undefined
  onToggle?: (() => void) | undefined
  menu?: MenuProps | undefined
  actions?: ReactNode
}

export const NavbarTreeRow = ({
  depth,
  label,
  icon,
  to,
  active,
  description,
  expandable,
  open,
  onToggle,
  menu,
  actions,
}: NavbarTreeRowProps): ReactNode => {
  const row = {
    ...menu,
    flex: 1,
    miw: 0,
    className: 'item-primary',
    leftSection: icon,
    label: (
      <Text size="sm" truncate>
        {label}
      </Text>
    ),
    description,
    variant: 'light',
  } as const

  return (
    <Group
      gap={0}
      wrap="nowrap"
      className="nav-item interactive-surface stretched"
      ml={depth * 12}
      mod={{ current: active }}
    >
      {expandable ? (
        <ActionIcon
          size="xs"
          aria-label={`${open ? 'Collapse' : 'Expand'} ${label}`}
          aria-expanded={open}
          onClick={onToggle}
        >
          <IconChevronRight size={14} aria-hidden style={{ transform: open ? 'rotate(90deg)' : undefined }} />
        </ActionIcon>
      ) : (
        <Box w={18} flex="none" />
      )}
      {to ? (
        <NavLink {...row} active={active} aria-current={active ? 'page' : undefined} component={Link} to={to} />
      ) : (
        <NavLink
          {...row}
          component="button"
          type="button"
          aria-expanded={open}
          {...(onToggle ? { onClick: onToggle } : {})}
        />
      )}
      {actions}
    </Group>
  )
}
