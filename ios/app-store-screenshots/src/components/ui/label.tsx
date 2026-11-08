'use client'
import { Root } from '@radix-ui/react-label'
import { cva } from 'class-variance-authority'
import type { ComponentProps, ReactNode } from 'react'
import { cn } from '@/lib/utils'

const labelVariants = cva('text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70')

function Label({ className, ...props }: ComponentProps<typeof Root>): ReactNode {
  return <Root className={cn(labelVariants(), className)} {...props} />
}

export { Label }
