import type { ScreenshotFontId } from './types'

export const DEFAULT_SCREENSHOT_FONT_ID: ScreenshotFontId = 'template-default'

// Family used for a font imported through the toolbar. The matching @font-face
// is injected into <head> by the editor (see screenshot-editor.tsx), so it sits
// in document.styleSheets where html-to-image can embed it into exports.
export const IMPORTED_FONT_FAMILY = 'ImportedScreenshotFont'

export const SCREENSHOT_FONTS: Record<ScreenshotFontId, { name: string; family: string }> = {
  // Inherit the editor's Inter (next/font in app/layout.tsx), which is what the
  // canvas rendered before fonts were selectable, so existing decks don't shift.
  'template-default': { name: 'Inter (default)', family: 'inherit' },
  'system-sans': {
    name: 'System Sans',
    family: 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  },
  'template-serif': { name: 'Georgia', family: "Georgia, 'Times New Roman', serif" },
  // Named system fonts ship with macOS; elsewhere they fall back to the listed
  // alternatives, so export on the machine whose fonts you designed with.
  'avenir-next': { name: 'Avenir Next', family: '"Avenir Next", Avenir, sans-serif' },
  'helvetica-neue': { name: 'Helvetica Neue', family: '"Helvetica Neue", Helvetica, Arial, sans-serif' },
  futura: { name: 'Futura', family: "Futura, 'Trebuchet MS', sans-serif" },
  baskerville: { name: 'Baskerville', family: "Baskerville, 'Baskerville Old Face', Georgia, serif" },
  palatino: { name: 'Palatino', family: "Palatino, 'Palatino Linotype', 'Book Antiqua', serif" },
  optima: { name: 'Optima', family: "Optima, Candara, 'Segoe UI', sans-serif" },
  'american-typewriter': { name: 'American Typewriter', family: '"American Typewriter", "Courier New", serif' },
  // Only offered once a font has been imported; the name shown comes from the file.
  'self-hosted': { name: 'Imported font', family: `"${IMPORTED_FONT_FAMILY}", sans-serif` },
}
