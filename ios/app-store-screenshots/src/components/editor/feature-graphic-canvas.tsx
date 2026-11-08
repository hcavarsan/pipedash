import type { CSSProperties, ReactNode } from 'react'
import { slideColors } from '@/lib/contrast'
import { img } from '@/lib/image-cache'
import { pickText } from '@/lib/locale'
import { shade } from '@/lib/scene-colors'
import type { Slide, Theme } from '@/lib/types'
import { slideFontScales } from '@/lib/typography'
import { Blob } from './canvas-background'
import type { EditHandlers } from './canvas-types'
import { EditableText } from './editable-text'
import { PlainImg } from './plain-img'

function AppIcon({
  cW,
  theme,
  appName,
  appIcon,
  fg,
}: {
  cW: number
  theme: Theme
  appName?: string | undefined
  appIcon?: string | undefined
  fg: string
}): ReactNode {
  const source = img(appIcon)
  const box: CSSProperties = {
    width: cW * 0.13,
    height: cW * 0.13,
    borderRadius: cW * 0.022,
    boxShadow: '0 4px 16px rgba(0,0,0,0.3)',
  }
  if (source) {
    return <PlainImg src={source} alt="" style={box} draggable={false} />
  }
  return (
    <div
      aria-hidden
      style={{
        ...box,
        background: `linear-gradient(135deg, ${theme.accent}55, ${theme.accent})`,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        color: fg,
        fontWeight: 800,
        fontSize: cW * 0.07,
      }}
    >
      {(appName || 'A').slice(0, 1).toUpperCase()}
    </div>
  )
}

export function FeatureGraphicCanvas({
  slide,
  cW,
  theme,
  locale,
  appName,
  appIcon,
  editable,
  edit,
}: {
  slide: Slide
  cW: number
  theme: Theme
  locale: string
  appName?: string | undefined
  appIcon?: string | undefined
  editable?: boolean | undefined
  edit?: EditHandlers | undefined
}): ReactNode {
  const { headlineScale, appNameScale } = slideFontScales(slide)
  const inverted = slide.inverted ?? true
  const bg = inverted ? theme.bgAlt : theme.bg
  const colors = slideColors(theme, { ...slide, inverted })
  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        position: 'relative',
        overflow: 'hidden',
        background:
          slide.backgroundColor || `linear-gradient(135deg, ${bg} 0%, ${shade(bg, -10)} 50%, ${theme.accent} 200%)`,
        display: 'flex',
        alignItems: 'center',
        padding: `0 ${cW * 0.06}px`,
        color: colors.fg,
      }}
    >
      <Blob cW={cW} color={theme.accent} x={70} y={20} size={50} opacity={0.45} />
      <div style={{ display: 'flex', alignItems: 'center', gap: cW * 0.03, zIndex: 2 }}>
        <AppIcon cW={cW} theme={theme} appName={appName} appIcon={appIcon} fg={colors.fg} />
        <div>
          <div style={{ fontSize: cW * 0.06 * appNameScale, fontWeight: 800, lineHeight: 1.05 }}>
            {appName || 'App'}
          </div>
          <EditableText
            value={pickText(slide.headline, locale)}
            editable={editable}
            multiline
            onChange={edit?.onHeadlineChange}
            onFocus={() => edit?.onSelectElement?.('caption')}
            style={{
              fontSize: cW * 0.028 * headlineScale,
              color: colors.fg,
              opacity: 0.85,
              marginTop: cW * 0.012,
              lineHeight: 1.25,
            }}
          />
        </div>
      </div>
    </div>
  )
}
