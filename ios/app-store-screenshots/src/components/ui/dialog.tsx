'use client'
import { Close, Content, Description, Overlay, Portal, Root, Title } from '@radix-ui/react-dialog'
import { X } from 'lucide-react'
import type { ComponentProps, ReactNode } from 'react'
import { cn } from '@/lib/utils'

const Dialog = Root

function DialogOverlay({ className, ...props }: ComponentProps<typeof Overlay>): ReactNode {
  return (
    <Overlay
      className={cn(
        'fixed inset-0 z-50 bg-black/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0',
        className
      )}
      {...props}
    />
  )
}

function DialogContent({ className, children, ...props }: ComponentProps<typeof Content>): ReactNode {
  return (
    <Portal>
      <DialogOverlay />
      <Content
        className={cn(
          'fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg duration-200 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 sm:rounded-lg',
          className
        )}
        {...props}
      >
        {children}
        <Close className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-hidden focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none">
          <X className="h-4 w-4" />
          <span className="sr-only">Close</span>
        </Close>
      </Content>
    </Portal>
  )
}

function DialogHeader({ className, ...props }: ComponentProps<'div'>): ReactNode {
  return <div className={cn('flex flex-col space-y-1.5 text-center sm:text-left', className)} {...props} />
}

function DialogTitle({ className, ...props }: ComponentProps<typeof Title>): ReactNode {
  return <Title className={cn('text-lg font-semibold leading-none tracking-tight', className)} {...props} />
}

function DialogDescription({ className, ...props }: ComponentProps<typeof Description>): ReactNode {
  return <Description className={cn('text-sm text-muted-foreground', className)} {...props} />
}

export { Dialog, DialogContent, DialogDescription, DialogHeader, DialogOverlay, DialogTitle }
