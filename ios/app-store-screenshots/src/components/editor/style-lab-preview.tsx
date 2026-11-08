'use client'
import type { ReactNode } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import { SCREENSHOT_FONTS } from '@/lib/fonts'
import { BACKDROPS, DECORATIONS } from '@/lib/scene'
import { directionName } from '@/lib/style-lab'
import { THEMES, themeById } from '@/lib/themes'
import type { Look, ProjectState } from '@/lib/types'
import { cn } from '@/lib/utils'
import { DeckCanvas } from './deck-canvas'

function describe(look: Look): string {
  const font = SCREENSHOT_FONTS[look.fontId]?.name.replace(' (default)', '') ?? look.fontId
  const backdrop = BACKDROPS.find((b) => b.id === look.scene.backdrop)?.name ?? look.scene.backdrop
  const decoration = DECORATIONS.find((d) => d.id === look.scene.decoration)?.name
  return [
    THEMES[look.themeId]?.name,
    font,
    `${backdrop}${look.scene.span ? ' (flowing)' : ''}`,
    look.scene.decoration !== 'none' ? decoration : null,
    look.scene.tilt ? `${look.scene.tilt}° tilt` : null,
    look.scene.headlineCase === 'upper' ? 'Uppercase' : null,
  ]
    .filter(Boolean)
    .join(' · ')
}

function DirectionBadge({ direction }: { direction: string }): ReactNode {
  return (
    <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
      {directionName(direction)}
    </span>
  )
}

export function LookRow({
  title,
  look,
  project,
  height,
  current,
  actions,
}: {
  title: string
  look: Look
  project: ProjectState
  height: number
  current?: boolean
  actions: ReactNode
}): ReactNode {
  return (
    <section
      aria-label={title}
      className={cn('rounded-lg border bg-card p-3 shadow-xs', current && 'border-dashed bg-card/60')}
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-semibold">{title}</h3>
            {!current && look.direction && <DirectionBadge direction={look.direction} />}
          </div>
          <p className="truncate text-[11px] text-muted-foreground">{describe(look)}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1">{actions}</div>
      </div>
      <div className="overflow-x-auto">
        <DeckPreview project={project} height={height} />
      </div>
    </section>
  )
}

function fontFamilyOf(project: ProjectState): string {
  return SCREENSHOT_FONTS[project.fontId || 'template-default'].family
}

/** A whole deck, scaled to `height`, exactly as the export would render it. */
export function DeckPreview({ project, height }: { project: ProjectState; height: number }): ReactNode {
  const slides = project.slidesByDevice[project.device] || []
  const { cW, cH } = getCanvas(project.device, project.orientation)
  const scale = height / cH
  return (
    <div
      className="relative overflow-hidden rounded-md"
      style={{ width: slides.length * cW * scale, height }}
      aria-hidden
    >
      <div style={{ position: 'absolute', left: 0, top: 0, transformOrigin: 'top left', transform: `scale(${scale})` }}>
        <DeckCanvas
          slides={slides}
          device={project.device}
          orientation={project.orientation}
          theme={themeById(project.themeId)}
          locale={project.locale}
          appName={project.appName}
          {...(project.appIcon ? { appIcon: project.appIcon } : {})}
          fontFamily={fontFamilyOf(project)}
          connectedCanvas={project.connectedCanvas}
          {...(project.scene ? { scene: project.scene } : {})}
          hideEmpty
        />
      </div>
    </div>
  )
}
