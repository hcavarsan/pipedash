'use client'
import { Download, FlaskConical, Redo2, RotateCcw, Undo2, UnfoldHorizontal } from 'lucide-react'
import { type ReactNode, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { DEVICE_LABEL } from '@/lib/device-specs'
import { THEMES, themeById } from '@/lib/themes'
import type { Device, ImportedFont, Orientation, Scene, ScreenshotFontId } from '@/lib/types'
import { ScenePlayground } from './scene-playground'
import { ToolbarDevicePicker } from './toolbar-device-picker'
import { ToolbarFontPicker } from './toolbar-font-picker'
import { SaveStatus } from './toolbar-status'
import { ThemeOption } from './toolbar-theme-option'

type Props = {
  appName: string
  setAppName: (v: string) => void
  themeId: string
  setThemeId: (v: string) => void
  connectedCanvas: boolean
  setConnectedCanvas: (v: boolean) => void
  scene: Scene | undefined
  setScene: (scene: Scene | undefined) => void
  onOpenStyleLab: () => void
  fontId: ScreenshotFontId
  setFontId: (v: ScreenshotFontId) => void
  importedFont?: ImportedFont | undefined
  setImportedFont: (font: ImportedFont) => void
  locale: string
  setLocale: (v: string) => void
  locales: string[]
  device: Device
  setDevice: (v: Device) => void
  orientation: Orientation
  setOrientation: (v: Orientation) => void
  onExport: () => void
  onResetAll: () => void
  onResetDevice: () => void
  onUndo: () => void
  onRedo: () => void
  canUndo: boolean
  canRedo: boolean
  exporting: string | null
  savedAt: number | null
  saveError: string | null
  onRetrySave?: (() => void) | undefined
  busy: boolean
}

export function Toolbar(props: Props): ReactNode {
  const [resetOpen, setResetOpen] = useState(false)
  const [importingFont, setImportingFont] = useState(false)
  const showLocale = props.locales.length > 1
  const deviceLabel = DEVICE_LABEL[props.device]
  const activeTheme = themeById(props.themeId)
  const localeOptions = props.locales.map((l) => (
    <SelectItem key={l} value={l}>
      {l.toUpperCase()}
    </SelectItem>
  ))
  const resetDeviceOnly = () => {
    setResetOpen(false)
    props.onResetDevice()
  }
  const resetAll = () => {
    setResetOpen(false)
    props.onResetAll()
  }

  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5 border-b bg-card/40 px-4 py-2">
      <Input
        value={props.appName}
        onChange={(e) => props.setAppName(e.target.value)}
        className="h-8 w-36 border-dashed text-sm font-semibold focus-visible:border-input focus-visible:border-solid focus-visible:bg-background"
        placeholder="App name"
        aria-label="App name"
        title="App name (click to edit)"
        disabled={props.busy}
      />

      <span aria-hidden className="mx-1 h-5 w-px bg-border" />

      <Button
        type="button"
        variant={props.connectedCanvas ? 'secondary' : 'outline'}
        size="sm"
        className="h-8 gap-1.5 px-2 text-xs"
        onClick={() => props.setConnectedCanvas(!props.connectedCanvas)}
        aria-pressed={props.connectedCanvas}
        title={
          props.connectedCanvas
            ? 'Connected canvas enabled'
            : 'Isolated screens; turn on to let elements cross screen edges'
        }
        disabled={props.busy}
      >
        <UnfoldHorizontal className="h-3.5 w-3.5" />
        {props.connectedCanvas ? 'Connected' : 'Isolated'}
      </Button>

      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 gap-1.5 px-2 text-xs"
        onClick={props.onOpenStyleLab}
        disabled={props.busy}
        title="Style Lab: try complete looks for this deck side by side"
      >
        <FlaskConical className="h-3.5 w-3.5" />
        Style Lab
      </Button>
      <ScenePlayground
        scene={props.scene}
        theme={activeTheme}
        disabled={props.busy}
        straightDevices={props.device.startsWith('duo-')}
        onChange={props.setScene}
      />

      <Select value={activeTheme.id} onValueChange={props.setThemeId} disabled={props.busy}>
        <SelectTrigger className="h-8 w-40 text-xs" title="Theme" aria-label="Theme">
          <SelectValue>
            <ThemeOption theme={activeTheme} />
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          {Object.values(THEMES).map((theme) => (
            <SelectItem key={theme.id} value={theme.id}>
              <ThemeOption theme={theme} />
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <ToolbarFontPicker
        fontId={props.fontId}
        setFontId={props.setFontId}
        importedFont={props.importedFont}
        setImportedFont={props.setImportedFont}
        importing={importingFont}
        setImporting={setImportingFont}
        busy={props.busy}
      />

      <span aria-hidden className="mx-1 h-5 w-px bg-border" />

      <ToolbarDevicePicker
        device={props.device}
        setDevice={props.setDevice}
        orientation={props.orientation}
        setOrientation={props.setOrientation}
        busy={props.busy}
      />

      {showLocale && (
        <Select value={props.locale} onValueChange={props.setLocale} disabled={props.busy}>
          <SelectTrigger className="h-8 w-20 text-xs" aria-label="Locale">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>{localeOptions}</SelectContent>
        </Select>
      )}

      <div className="ml-auto flex shrink-0 items-center gap-2">
        <SaveStatus savedAt={props.savedAt} saveError={props.saveError} />
        {props.saveError && props.onRetrySave && (
          <Button variant="ghost" size="sm" disabled={props.busy} onClick={props.onRetrySave}>
            Retry save
          </Button>
        )}
        <span aria-hidden className="h-5 w-px bg-border" />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={props.onUndo}
          title="Undo (⌘Z)"
          aria-label="Undo"
          disabled={props.busy || !props.canUndo}
        >
          <Undo2 className="h-4 w-4" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={props.onRedo}
          title="Redo (⌘⇧Z)"
          aria-label="Redo"
          disabled={props.busy || !props.canRedo}
        >
          <Redo2 className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={() => setResetOpen(true)}
          title="Reset screens to defaults"
          aria-label="Reset"
          disabled={props.busy}
        >
          <RotateCcw className="h-4 w-4" />
        </Button>
        <Button
          onClick={props.onExport}
          disabled={!!props.exporting || importingFont}
          size="sm"
          className="h-8"
          title="Export every size × locale for this device as a zip"
        >
          <Download className="h-4 w-4" />
          {props.exporting ? `Exporting ${props.exporting}` : 'Export bundle'}
        </Button>
      </div>

      <Dialog open={resetOpen} onOpenChange={setResetOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Reset to defaults?</DialogTitle>
            <DialogDescription>
              Choose whether to reset just <span className="font-medium">{deviceLabel}</span> or every device deck. Your
              canvas edits, uploaded screenshots, and copy will be lost.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-wrap justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setResetOpen(false)}>
              Cancel
            </Button>
            <Button variant="outline" size="sm" onClick={resetDeviceOnly}>
              Reset {deviceLabel} only
            </Button>
            <Button variant="destructive" size="sm" onClick={resetAll}>
              Reset all devices
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
