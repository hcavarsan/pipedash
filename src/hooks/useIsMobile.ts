import { useMediaQuery } from '@mantine/hooks'

export const MOBILE_BREAKPOINT = 768

export const useIsMobile = () => {
  const isMobile = useMediaQuery(`(max-width: ${MOBILE_BREAKPOINT}px)`)

  return { isMobile, isDesktop: !isMobile }
}
