import { useMediaQuery } from '@mantine/hooks'

import { isMobilePlatform } from '../utils/platform'

export const MOBILE_BREAKPOINT = 768

export const useIsMobile = () => {
  const isMobile = useMediaQuery(`(max-width: ${MOBILE_BREAKPOINT}px)`, undefined, { getInitialValueInEffect: false })

  return { isMobile, isDesktop: !isMobile }
}

export const useMobileShell = (): boolean => {
  const { isMobile } = useIsMobile()

  return isMobilePlatform() && isMobile
}

export const isMobileShell = (): boolean =>
  isMobilePlatform() && window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT}px)`).matches
