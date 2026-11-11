'use client'
import { Check, Dices, Download, Lock, LockOpen, Shuffle, Star, Trash2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { LOCK_KEYS, LOCK_LABEL, type LockKey, type Locks } from '@/lib/style-lab'
import type { Look } from '@/lib/types'
import { cn } from '@/lib/utils'

function SaveButton({ saved, onClick, name }: { saved: boolean; onClick: () => void; name: string }): ReactNode {
  return (
    <Button
      type="button"
      size="sm"
      variant="ghost"
      className="h-7 gap-1 px-2 text-xs"
      onClick={onClick}
      aria-pressed={saved}
      aria-label={`${saved ? 'Unsave' : 'Save'} ${name}`}
      title={saved ? 'Remove from saved looks' : 'Keep this look for later'}
    >
      <Star className={cn('h-3.5 w-3.5', saved && 'fill-amber-400 text-amber-500')} />
      {saved ? 'Saved' : 'Save'}
    </Button>
  )
}

export function GeneratedLookActions({
  look,
  title,
  saved,
  onRemix,
  onToggleSaved,
  onApply,
}: {
  look: Look
  title: string
  saved: boolean
  onRemix: () => void
  onToggleSaved: (look: Look) => void
  onApply: (look: Look) => void
}): ReactNode {
  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        className="h-7 gap-1 px-2 text-xs"
        onClick={onRemix}
        aria-label={`Remix ${title}`}
        title="Another take on this direction"
      >
        <Dices className="h-3.5 w-3.5" />
        Remix
      </Button>
      <SaveButton saved={saved} onClick={() => onToggleSaved(look)} name={title} />
      <Button
        type="button"
        size="sm"
        className="h-7 gap-1 px-2.5 text-xs"
        onClick={() => onApply(look)}
        aria-label={`Apply ${title}`}
      >
        <Check className="h-3.5 w-3.5" />
        Apply
      </Button>
    </>
  )
}

export function SavedLookActions({
  look,
  onRemove,
  onApply,
}: {
  look: Look
  onRemove: (look: Look) => void
  onApply: (look: Look) => void
}): ReactNode {
  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        className="h-7 w-7 p-0 hover:text-destructive"
        onClick={() => onRemove(look)}
        aria-label={`Remove saved look ${look.name}`}
        title="Remove from saved looks"
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
      <Button
        type="button"
        size="sm"
        className="h-7 gap-1 px-2.5 text-xs"
        onClick={() => onApply(look)}
        aria-label={`Apply saved look ${look.name}`}
      >
        <Check className="h-3.5 w-3.5" />
        Apply
      </Button>
    </>
  )
}

function lockTitle(key: LockKey, unavailable: boolean, on: boolean): string {
  if (unavailable) {
    return 'This device keeps its own layouts'
  }
  return `${LOCK_LABEL[key].hint}. ${on ? 'Kept from your deck.' : 'Varies between looks.'}`
}

export function StyleLabControls({
  locks,
  rearrange,
  allLocked,
  exporting,
  canExport,
  onToggleLock,
  onExport,
  onShuffle,
}: {
  locks: Locks
  rearrange: boolean
  allLocked: boolean
  exporting: boolean
  canExport: boolean
  onToggleLock: (key: LockKey) => void
  onExport: () => void
  onShuffle: () => void
}): ReactNode {
  return (
    <div className="flex flex-wrap items-center gap-2 border-b px-5 py-2.5">
      <span className="text-xs font-medium text-muted-foreground">Keep</span>
      {LOCK_KEYS.map((key) => {
        const unavailable = key === 'layout' && !rearrange
        const on = locks[key] || unavailable
        return (
          <Button
            key={key}
            type="button"
            size="sm"
            variant={on ? 'secondary' : 'outline'}
            className="h-7 gap-1.5 px-2.5 text-xs"
            aria-pressed={on}
            disabled={unavailable}
            title={lockTitle(key, unavailable, on)}
            onClick={() => onToggleLock(key)}
          >
            {on ? <Lock className="h-3 w-3" /> : <LockOpen className="h-3 w-3" />}
            {LOCK_LABEL[key].name}
          </Button>
        )
      })}
      <div className="ml-auto flex items-center gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8 gap-1.5"
          onClick={onExport}
          disabled={exporting || !canExport}
          title="Download one image comparing your deck with these looks"
        >
          <Download className="h-3.5 w-3.5" />
          {exporting ? 'Exporting…' : 'Export comparison'}
        </Button>
        <Button
          type="button"
          size="sm"
          className="h-8 gap-1.5"
          onClick={onShuffle}
          disabled={allLocked}
          title={allLocked ? 'Unlock something to shuffle it' : 'Four new looks'}
        >
          <Shuffle className="h-3.5 w-3.5" />
          Shuffle
        </Button>
      </div>
    </div>
  )
}
