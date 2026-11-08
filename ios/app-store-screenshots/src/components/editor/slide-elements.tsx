'use client'
import type { ReactNode } from 'react'
import { calloutAvailable, getSlideGeometry, rectFor } from '@/lib/canvas-geometry'
import { isDuoDevice } from '@/lib/frame-assets'
import { resolveScreenshot } from '@/lib/locale'
import type { Device, Orientation, Scene, Slide, Theme } from '@/lib/types'
import { CanvasCalloutElement } from './canvas-callout-element'
import { CanvasCaptionElement } from './canvas-caption-element'
import { CanvasDeviceElement } from './canvas-device-element'
import { CanvasImageElement } from './canvas-image-element'
import { CanvasTextElement } from './canvas-text-element'
import type { ElementLayer } from './canvas-types'

export function SlideElements({
  slide,
  scene,
  device,
  orientation,
  theme,
  locale,
  hideEmpty,
  layer,
}: {
  slide: Slide
  scene: Scene
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  hideEmpty?: boolean | undefined
  layer: ElementLayer
}): ReactNode {
  const screenshot = resolveScreenshot(slide.screenshot, locale)
  const screenshotSecondary = resolveScreenshot(slide.screenshotSecondary, locale)
  const { cW, cH, Frame, frameAspect, defaults, typeUnit } = getSlideGeometry(slide, device, orientation)
  const captionRect = rectFor('caption', slide, defaults)
  const deviceRect = rectFor('device', slide, defaults)
  const secondaryRect = rectFor('deviceSecondary', slide, defaults)
  const calloutRect =
    slide.callout && deviceRect && calloutAvailable(slide, device) ? rectFor('callout', slide, defaults) : undefined

  const duoDevice = isDuoDevice(device) ? device : null
  const deviceProps = { layer, slide, scene, theme, Frame, frameAspect, duoDevice, cW, hideEmpty }
  // Another display's capture can't stand in for the companion's own.
  const secondarySrc = duoDevice ? screenshotSecondary : screenshotSecondary || screenshot
  const secondaryStyle = duoDevice ? undefined : { opacity: 0.85 }

  return (
    <>
      {secondaryRect && (
        <CanvasDeviceElement
          {...deviceProps}
          slot="deviceSecondary"
          rect={secondaryRect}
          src={secondarySrc}
          extraStyle={secondaryStyle}
        />
      )}
      {deviceRect && <CanvasDeviceElement {...deviceProps} slot="device" rect={deviceRect} src={screenshot} />}
      {captionRect && (
        <CanvasCaptionElement
          layer={layer}
          slide={slide}
          scene={scene}
          theme={theme}
          locale={locale}
          rect={captionRect}
          cW={cW}
          cH={cH}
          typeUnit={typeUnit}
        />
      )}
      {calloutRect && deviceRect && slide.callout && (
        <CanvasCalloutElement
          layer={layer}
          slide={slide}
          theme={theme}
          callout={slide.callout}
          rect={calloutRect}
          deviceWidth={deviceRect.width}
          src={screenshot}
          cW={cW}
          hideEmpty={hideEmpty}
        />
      )}
      {(slide.imageElements || []).map((element, index) => (
        <CanvasImageElement key={element.id} layer={layer} element={element} index={index} />
      ))}
      {(slide.textElements || []).map((element, index) => (
        <CanvasTextElement
          key={element.id}
          layer={layer}
          slide={slide}
          theme={theme}
          locale={locale}
          element={element}
          index={index}
          cW={cW}
          cH={cH}
        />
      ))}
    </>
  )
}
