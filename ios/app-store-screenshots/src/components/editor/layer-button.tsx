import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

export function LayerButton({
  disabled,
  active,
  onClick,
  label,
  children,
}: {
  disabled: boolean
  active?: boolean
  onClick: () => void
  label: string
  children: ReactNode
}): ReactNode {
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className={cn('h-7 px-0', active && 'border-foreground/40 bg-accent text-accent-foreground')}
      disabled={disabled}
      aria-pressed={active}
      onClick={onClick}
      title={label}
      aria-label={label}
    >
      {children}
    </Button>
  )
}
