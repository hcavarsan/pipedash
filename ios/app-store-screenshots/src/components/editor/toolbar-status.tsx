'use client'
import { AlertTriangle, Check, Cloud } from 'lucide-react'
import { type ReactNode, useEffect, useState } from 'react'

function savedLabel(seconds: number): string {
  if (seconds < 5) {
    return 'saved'
  }
  if (seconds < 60) {
    return `saved ${seconds}s ago`
  }
  if (seconds < 3600) {
    return `saved ${Math.round(seconds / 60)}m ago`
  }
  return `saved ${Math.round(seconds / 3600)}h ago`
}

export function SaveStatus({ savedAt, saveError }: { savedAt: number | null; saveError: string | null }): ReactNode {
  const [, setTick] = useState(0)
  useEffect(() => {
    const t = setInterval(() => setTick((x) => x + 1), 60_000)
    return () => clearInterval(t)
  }, [])

  if (saveError) {
    return (
      <span className="flex items-center gap-1 text-xs text-destructive" title={saveError}>
        <AlertTriangle className="h-3.5 w-3.5" /> save failed
      </span>
    )
  }

  if (!savedAt) {
    return (
      <span className="flex items-center gap-1 text-xs text-muted-foreground" title="Not saved yet">
        <Cloud className="h-3.5 w-3.5" /> <span className="hidden 2xl:inline">not saved yet</span>
      </span>
    )
  }
  const label = savedLabel(Math.max(0, Math.round((Date.now() - savedAt) / 1000)))
  return (
    <span className="flex items-center gap-1 text-xs text-muted-foreground" title={`Project ${label}`}>
      <Check className="h-3.5 w-3.5 text-green-500" /> <span className="hidden 2xl:inline">{label}</span>
    </span>
  )
}
