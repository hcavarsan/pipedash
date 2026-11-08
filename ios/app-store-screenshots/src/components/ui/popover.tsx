'use client'
import { Content, Portal, Root, Trigger } from '@radix-ui/react-popover'
import type { ComponentProps, ReactNode } from 'react'
import { cn } from '@/lib/utils'

const Popover = Root
const PopoverTrigger = Trigger

function PopoverContent({
  className,
  align = 'start',
  sideOffset = 6,
  ...props
}: ComponentProps<typeof Content>): ReactNode {
  return (
    <Portal>
      <Content
        align={align}
        sideOffset={sideOffset}
        className={cn(
          'z-50 rounded-lg border bg-popover p-3 text-popover-foreground shadow-lg outline-hidden data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0',
          className
        )}
        {...props}
      />
    </Portal>
  )
}

export { Popover, PopoverContent, PopoverTrigger }
