import { useMediaQuery } from '@mantine/hooks'

import { isMobilePlatform } from '../utils/platform'

export const MOBILE_BREAKPOINT = 768

const NARROW_QUERY = `(max-width: ${MOBILE_BREAKPOINT}px)`

export const isPhone = (): boolean => /iPhone|iPod|Android.+Mobile/.test(navigator.userAgent)

interface IsMobileResult {
  isMobile: boolean
  isDesktop: boolean
}

export const useIsMobile = (): IsMobileResult => {
  const narrow = useMediaQuery(NARROW_QUERY, undefined, { getInitialValueInEffect: false })
  const isMobile = isPhone() || narrow

  return { isMobile, isDesktop: !isMobile }
}

export const useMobileShell = (): boolean => {
  const { isMobile } = useIsMobile()

  return isPhone() || (isMobilePlatform() && isMobile)
}

export const isMobileShell = (): boolean => isPhone() || (isMobilePlatform() && window.matchMedia(NARROW_QUERY).matches)
