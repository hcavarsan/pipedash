import type { ReactNode, RefObject } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import { DEFAULT_SCREENSHOT_FONT_ID, SCREENSHOT_FONTS } from '@/lib/fonts'
import { themeById } from '@/lib/themes'
import type { ProjectState } from '@/lib/types'
import { DeckCanvas } from './deck-canvas'

type Props = {
  project: ProjectState
  targetRef: RefObject<HTMLDivElement | null>
  slideIndex: number
  localeOverride: string | null
}

// Off-screen export container — full-resolution canvases for html-to-image.
export function ExportCanvas({ project, targetRef, slideIndex, localeOverride }: Props): ReactNode {
  const slides = project.slidesByDevice[project.device] || []
  const { cW, cH } = getCanvas(project.device, project.orientation)
  return (
    <div
      aria-hidden
      style={{
        position: 'absolute',
        left: -99999,
        top: 0,
        pointerEvents: 'none',
      }}
    >
      {slides.length > 0 && (
        <div
          ref={targetRef}
          style={{
            width: cW,
            height: cH,
            overflow: 'hidden',
            position: 'absolute',
            left: -99999,
            top: 0,
          }}
        >
          <div
            style={{
              position: 'absolute',
              left: -slideIndex * cW,
              top: 0,
              width: cW * slides.length,
              height: cH,
            }}
          >
            <DeckCanvas
              slides={slides}
              device={project.device}
              orientation={project.orientation}
              theme={themeById(project.themeId)}
              locale={localeOverride ?? project.locale}
              appName={project.appName}
              appIcon={project.appIcon}
              fontFamily={SCREENSHOT_FONTS[project.fontId || DEFAULT_SCREENSHOT_FONT_ID].family}
              connectedCanvas={project.connectedCanvas}
              scene={project.scene}
              hideEmpty
            />
          </div>
        </div>
      )}
    </div>
  )
}
