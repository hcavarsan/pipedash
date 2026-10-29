import { Image } from '@mantine/core'
import type { ReactNode } from 'react'

import { withBasePath } from '../../../lib/basePath'

const LOGO_WIDTHS = [28, 56, 84, 112, 168]
const SRC_SET = LOGO_WIDTHS.map((width) => `${withBasePath(`/app-logo/${width}.png`)} ${width}w`).join(', ')

interface AppLogoProps {
  size: number | { base: number; sm?: number }
  alt?: string
}

export const AppLogo = ({ size, alt = '' }: AppLogoProps): ReactNode => {
  const largest = typeof size === 'number' ? size : Math.max(size.base, size.sm ?? 0)

  return (
    <Image
      src={withBasePath('/app-logo/56.png')}
      srcSet={SRC_SET}
      sizes={`${largest}px`}
      alt={alt}
      w={size}
      h={size}
      fit="contain"
      className="app-logo"
    />
  )
}
