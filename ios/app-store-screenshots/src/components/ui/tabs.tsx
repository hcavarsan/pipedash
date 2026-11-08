'use client'
import { List, Root, Trigger } from '@radix-ui/react-tabs'
import type { ComponentProps, ReactNode } from 'react'
import { cn } from '@/lib/utils'

const Tabs = Root

function TabsList({ className, ...props }: ComponentProps<typeof List>): ReactNode {
  return (
    <List
      className={cn(
        'inline-flex h-9 items-center justify-center rounded-lg bg-muted p-1 text-muted-foreground',
        className
      )}
      {...props}
    />
  )
}

function TabsTrigger({ className, ...props }: ComponentProps<typeof Trigger>): ReactNode {
  return (
    <Trigger
      className={cn(
        'inline-flex items-center justify-center whitespace-nowrap rounded-md px-3 py-1 text-sm font-medium ring-offset-background transition-all focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm',
        className
      )}
      {...props}
    />
  )
}

export { Tabs, TabsList, TabsTrigger }
