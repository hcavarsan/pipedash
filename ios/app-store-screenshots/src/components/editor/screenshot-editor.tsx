'use client'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { Toaster, toast } from 'sonner'
import { deviceForOrientation, orientationOf } from '@/lib/device-specs'
import { DEFAULT_SCREENSHOT_FONT_ID, SCREENSHOT_FONTS } from '@/lib/fonts'
import { useProject } from '@/lib/storage'
import { hasTheme, themeById } from '@/lib/themes'
import type { Device, Look, Orientation, Patch, ProjectState } from '@/lib/types'
import { ExportCanvas } from './export-canvas'
import { Inspector } from './inspector'
import { PreviewStage } from './preview-stage'
import { Sidebar } from './sidebar'
import { StyleLab } from './style-lab'
import { Toolbar } from './toolbar'
import { useEditorAssets } from './use-editor-assets'
import { useEditorShortcuts } from './use-editor-shortcuts'
import { useEditorSlideActions } from './use-editor-slide-actions'
import { useScreenshotExport } from './use-screenshot-export'

function EditorLoading(): ReactNode {
  return (
    <div className="flex h-screen items-center justify-center">
      <div className="flex flex-col items-center gap-2 text-muted-foreground">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-current border-t-transparent" />
        <p className="text-sm">Loading editor…</p>
      </div>
    </div>
  )
}

export function ScreenshotEditor(): ReactNode {
  const { state, setState, hydrated, savedAt, saveError, retrySave, reset, resetDevice, undo, redo, canUndo, canRedo } =
    useProject()
  const [styleLabOpen, setStyleLabOpen] = useState(false)
  // The latest committed project, for actions that must check nothing changed since.
  const latestState = useRef(state)
  latestState.current = state

  const slides = useEditorSlideActions({ state, setState })
  const { currentSlides, activeSlide, selectedElement } = slides
  const theme = themeById(state.themeId)
  const fontId = state.fontId || DEFAULT_SCREENSHOT_FONT_ID
  const fontFamily = SCREENSHOT_FONTS[fontId].family
  const ready = useEditorAssets(state, hydrated)
  const exporter = useScreenshotExport(state, currentSlides, fontId, fontFamily)
  useEditorShortcuts({
    exportInProgress: exporter.exportInProgress,
    slides: currentSlides,
    activeSlide,
    selectSlide: slides.selectSlide,
    clearSelection: slides.clearSelection,
    duplicateSlide: slides.duplicateSlide,
    deleteSlide: slides.deleteSlide,
    undo,
    redo,
  })

  useEffect(() => {
    if (hydrated && state.themeId && !hasTheme(state.themeId)) {
      toast.warning('Using fallback theme', {
        description: `Theme "${state.themeId}" is not defined in src/lib/themes.ts.`,
        duration: 8000,
      })
    }
  }, [hydrated, state.themeId])

  // Surface storage failures (quota exceeded etc.) so the user knows their work isn't safe.
  useEffect(() => {
    if (saveError) {
      toast.error("Couldn't load or save project file", {
        description: saveError,
        duration: 8000,
      })
    }
  }, [saveError])

  if (!ready) {
    return <EditorLoading />
  }

  const busy = !!exporter.exporting
  const patchProject = (patch: Patch<ProjectState>) => setState((p) => Object.assign({}, p, patch))
  // Navigation (device, orientation, locale) and the look library aren't deck edits: no undo step.
  const navigate = (patch: Patch<ProjectState>) => setState((p) => Object.assign({}, p, patch), { history: false })

  const changeDevice = (device: Device) => {
    if (device !== state.device) {
      slides.resetSelection()
    }
    setState((p) => ({ ...p, device, orientation: orientationOf(device, p.orientation) }), { history: false })
  }

  // A Duo orientation is its own deck: switching opens it rather than
  // re-laying out the current one, so both compositions survive.
  const changeOrientation = (orientation: Orientation) => {
    if (deviceForOrientation(state.device, orientation) !== state.device) {
      slides.resetSelection()
    }
    setState((p) => ({ ...p, orientation, device: deviceForOrientation(p.device, orientation) }), {
      history: false,
    })
  }

  const resetAll = () => {
    reset()
    slides.resetSelection()
    toast.success('Reset all devices to defaults')
  }

  const resetCurrentDevice = () => {
    resetDevice(state.device)
    slides.resetSelection()
    toast.success(`Reset ${state.device} to defaults`)
  }

  const undoLook = (applied: ProjectState) => {
    // Only undo the look itself, never an edit made after it.
    if (latestState.current === applied) {
      undo()
    } else {
      toast('Use the toolbar Undo', { description: "You've edited the deck since applying this look." })
    }
  }

  const applyLook = (next: ProjectState, look: Look) => {
    setState(next, { coalesce: false })
    slides.clearSelection()
    toast.success(`Applied ${look.name}`, {
      action: { label: 'Undo', onClick: () => undoLook(next) },
      duration: 8000,
    })
  }

  const exportState = exporter.exportProject ?? state

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-background">
      <Toaster position="bottom-center" richColors closeButton />
      <Toolbar
        appName={state.appName}
        setAppName={(appName) => patchProject({ appName })}
        themeId={state.themeId}
        setThemeId={(themeId) => patchProject({ themeId })}
        connectedCanvas={state.connectedCanvas}
        setConnectedCanvas={(connectedCanvas) => patchProject({ connectedCanvas })}
        scene={state.scene}
        setScene={(scene) => patchProject({ scene })}
        onOpenStyleLab={() => setStyleLabOpen(true)}
        fontId={fontId}
        setFontId={(nextFontId) => patchProject({ fontId: nextFontId })}
        importedFont={state.importedFont}
        setImportedFont={(importedFont) => patchProject({ fontId: 'self-hosted', importedFont })}
        locale={state.locale}
        setLocale={(locale) => navigate({ locale })}
        locales={state.locales}
        device={state.device}
        setDevice={changeDevice}
        orientation={state.orientation}
        setOrientation={changeOrientation}
        onExport={exporter.exportAll}
        onResetAll={resetAll}
        onResetDevice={resetCurrentDevice}
        onUndo={undo}
        onRedo={redo}
        canUndo={canUndo}
        canRedo={canRedo}
        exporting={exporter.exporting}
        savedAt={savedAt}
        saveError={saveError}
        onRetrySave={retrySave}
        busy={busy}
      />

      <div
        inert={busy}
        aria-busy={busy}
        className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:flex-row lg:overflow-hidden"
      >
        <aside className="lg:w-72 w-full shrink-0 border-r bg-card lg:max-h-none max-h-64 overflow-hidden">
          <Sidebar
            slides={currentSlides}
            activeId={activeSlide?.id || null}
            device={state.device}
            orientation={state.orientation}
            theme={theme}
            locale={state.locale}
            appName={state.appName}
            appIcon={state.appIcon}
            fontFamily={fontFamily}
            connectedCanvas={state.connectedCanvas}
            scene={state.scene}
            disabled={busy}
            onReorder={slides.reorderSlides}
            onSelect={slides.selectSlide}
            onDelete={slides.deleteSlide}
            onDuplicate={slides.duplicateSlide}
            onAdd={slides.addSlide}
          />
        </aside>

        <main className="flex h-104 shrink-0 items-stretch overflow-hidden lg:h-auto lg:min-h-0 lg:flex-1">
          {activeSlide && currentSlides.length > 0 ? (
            <PreviewStage
              slides={currentSlides}
              activeSlideId={activeSlide.id}
              device={state.device}
              orientation={state.orientation}
              theme={theme}
              locale={state.locale}
              appName={state.appName}
              appIcon={state.appIcon}
              fontFamily={fontFamily}
              connectedCanvas={state.connectedCanvas}
              scene={state.scene}
              selectedElement={selectedElement}
              onActiveSlideChange={slides.selectSlide}
              onLabelChange={(slide, v) => slides.patchLocalized(slide, 'label', v)}
              onHeadlineChange={(slide, v) => slides.patchLocalized(slide, 'headline', v)}
              onTextElementTextChange={slides.patchTextElementText}
              onElementChange={slides.patchElementTransform}
              onSelectElement={slides.selectElement}
            />
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center text-sm text-muted-foreground">
              <p className="font-medium text-foreground">No screen selected</p>
              <p>Add a screen on the left to get started.</p>
            </div>
          )}
        </main>

        <aside className="lg:w-80 w-full shrink-0 border-l bg-card lg:max-h-none max-h-96 overflow-hidden">
          {activeSlide ? (
            <Inspector
              key={`${state.device}:${activeSlide.id}`}
              slide={activeSlide}
              device={state.device}
              orientation={state.orientation}
              theme={theme}
              locale={state.locale}
              appIcon={state.appIcon}
              onAppIconChange={(appIcon) => patchProject({ appIcon })}
              selectedElementId={selectedElement?.elementId ?? null}
              onChange={(patch) => slides.patchSlide(activeSlide.id, patch)}
              onUpdate={(update) => slides.updateSlide(activeSlide.id, update)}
              onSelectElement={slides.selectActiveElement}
            />
          ) : (
            <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-sm text-muted-foreground">
              <p className="font-medium text-foreground">Nothing to inspect</p>
              <p className="text-xs">Screen settings will appear here once you add or select one.</p>
            </div>
          )}
        </aside>
      </div>

      <StyleLab
        open={styleLabOpen}
        onOpenChange={setStyleLabOpen}
        state={state}
        onApply={applyLook}
        onSavedLooksChange={(savedLooks) => navigate({ savedLooks: savedLooks.length ? savedLooks : undefined })}
      />

      <ExportCanvas
        project={exportState}
        targetRef={exporter.exportRef}
        slideIndex={exporter.exportSlideIndex}
        localeOverride={exporter.exportLocaleOverride}
      />
    </div>
  )
}
