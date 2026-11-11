'use client'
import { Content, Item, Label, Portal, Root, Separator, Trigger } from '@radix-ui/react-dropdown-menu'
import type { ComponentProps, ReactNode } from 'react'
import { cn } from '@/lib/utils'

const DropdownMenu = Root
const DropdownMenuTrigger = Trigger

function DropdownMenuContent({ className, sideOffset = 4, ...props }: ComponentProps<typeof Content>): ReactNode {
  return (
    <Portal>
      <Content
        sideOffset={sideOffset}
        className={cn(
          'z-50 min-w-32 overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0',
          className
        )}
        {...props}
      />
    </Portal>
  )
}

function DropdownMenuItem({ className, ...props }: ComponentProps<typeof Item>): ReactNode {
  return (
    <Item
      className={cn(
        'relative flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-hidden transition-colors focus:bg-accent focus:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-50',
        className
      )}
      {...props}
    />
  )
}

function DropdownMenuSeparator({ className, ...props }: ComponentProps<typeof Separator>): ReactNode {
  return <Separator className={cn('-mx-1 my-1 h-px bg-muted', className)} {...props} />
}

function DropdownMenuLabel({ className, ...props }: ComponentProps<typeof Label>): ReactNode {
  return <Label className={cn('px-2 py-1.5 text-sm font-semibold', className)} {...props} />
}

export {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
}
