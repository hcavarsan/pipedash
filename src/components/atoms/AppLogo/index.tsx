import { Image, type MantineSize, type StyleProp } from '@mantine/core'

interface AppLogoProps {
  size: StyleProp<number | MantineSize>
  alt?: string
}

export const AppLogo = ({ size, alt = '' }: AppLogoProps) => (
  <Image src="/app-icon.png" alt={alt} w={size} h={size} fit="contain" className="app-logo" />
)
