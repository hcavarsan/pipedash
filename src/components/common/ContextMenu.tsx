import { Box, Menu, Text } from '@mantine/core'
import { type KeyboardEvent, type MouseEvent, type ReactNode, useEffect, useRef } from 'react'
import { useLocation } from 'react-router-dom'
import { create } from 'zustand'

import { useMobileShell } from '../../hooks/useIsMobile'

export type ContextMenuItem =
  | {
      label: string
      icon?: ReactNode | undefined
      onClick: () => void
      color?: 'red' | undefined
      disabled?: boolean | undefined
      shortcut?: string | undefined
    }
  | { divider: true }

interface ContextMenuState {
  menu: { x: number; y: number; items: ContextMenuItem[]; trigger: HTMLElement } | null
  open: (menu: NonNullable<ContextMenuState['menu']>) => void
  close: () => void
}

const useContextMenuStore = create<ContextMenuState>((set) => ({
  menu: null,
  open: (menu) => set({ menu }),
  close: () => {
    if (useContextMenuStore.getState().menu) {
      set({ menu: null })
    }
  },
}))

export function useContextMenu(): (items: () => ContextMenuItem[]) => {
  onContextMenu: (event: MouseEvent) => void
  onKeyDown: (event: KeyboardEvent) => void
} {
  const mobileShell = useMobileShell()
  const { key } = useLocation()

  useEffect(() => {
    if (key) {
      useContextMenuStore.getState().close()
    }

    return () => useContextMenuStore.getState().close()
  }, [key])

  return (items) => {
    const open = (event: MouseEvent | KeyboardEvent, x: number, y: number) => {
      if (mobileShell || !(event.currentTarget instanceof HTMLElement)) {
        return
      }

      const entries = items()

      if (!entries.some((item) => !('divider' in item))) {
        return
      }

      event.preventDefault()
      event.stopPropagation()
      useContextMenuStore.getState().open({ x, y, items: entries, trigger: event.currentTarget })
    }

    return {
      onContextMenu: (event) => open(event, event.clientX, event.clientY),
      onKeyDown: (event) => {
        if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
          const anchor = event.target instanceof HTMLElement ? event.target : event.currentTarget
          const rect = anchor.getBoundingClientRect()

          open(event, rect.left, rect.bottom)
        }
      },
    }
  }
}

export function ContextMenuHost() {
  const menu = useContextMenuStore((state) => state.menu)
  const close = useContextMenuStore((state) => state.close)
  const dropdownRef = useRef<HTMLDivElement>(null)
  const firstItem = menu?.items.find((item) => 'label' in item && !item.disabled)

  useEffect(() => {
    if (!menu) {
      return
    }

    window.addEventListener('scroll', close, true)
    window.addEventListener('resize', close)

    return () => {
      window.removeEventListener('scroll', close, true)
      window.removeEventListener('resize', close)
    }
  }, [menu, close])

  return (
    <Menu
      opened={menu !== null}
      onClose={() => {
        const focus = document.activeElement

        if (!focus || focus === document.body || dropdownRef.current?.contains(focus)) {
          menu?.trigger.focus({ preventScroll: true })
        }
        close()
      }}
      returnFocus={false}
      position="bottom-start"
      offset={0}
      floatingStrategy="fixed"
      zIndex={500}
      withInitialFocusPlaceholder={false}
    >
      <Menu.Target>
        <Box
          aria-hidden
          tabIndex={-1}
          style={{ position: 'fixed', left: menu?.x ?? 0, top: menu?.y ?? 0, width: 0, height: 0 }}
        />
      </Menu.Target>
      <Menu.Dropdown ref={dropdownRef} aria-label="Actions">
        {menu?.items.map((item, index) =>
          'divider' in item ? (
            <Menu.Divider
              key={`divider-${menu.items.slice(index + 1).find((entry) => 'label' in entry)?.label ?? 'end'}`}
            />
          ) : (
            <Menu.Item
              key={item.label}
              leftSection={item.icon}
              rightSection={
                item.shortcut ? (
                  <Text size="xs" c="dimmed">
                    {item.shortcut}
                  </Text>
                ) : null
              }
              disabled={item.disabled ?? false}
              {...(item.color === undefined ? {} : { color: item.color })}
              {...(item === firstItem ? { 'data-autofocus': true } : {})}
              onClick={item.onClick}
            >
              {item.label}
            </Menu.Item>
          )
        )}
      </Menu.Dropdown>
    </Menu>
  )
}
