import { Image, Indicator, Text, ThemeIcon } from '@mantine/core'

import { withBasePath } from '../../../lib/basePath'

interface ProviderIconProps {
  name: string
  icon: string | null
  size: number
  failed?: boolean
}

const initials = (name: string) => {
  const words = name.match(/[\p{L}\p{N}]+/gu) ?? ['?']

  return (words.length > 1 ? `${words[0][0]}${words[1][0]}` : words[0].slice(0, 2)).toUpperCase()
}

export const ProviderIcon = ({ name, icon, size, failed = false }: ProviderIconProps) => (
  <Indicator color="red" size={Math.max(8, size / 3)} offset={1} withBorder disabled={!failed}>
    <ThemeIcon size={size} variant="transparent" color="gray" aria-hidden>
      {icon ? (
        <Image src={withBasePath(icon)} alt="" w={size * 0.65} h={size * 0.65} fit="contain" />
      ) : (
        <Text fz={size * 0.4} fw={700} lh={1}>
          {initials(name)}
        </Text>
      )}
    </ThemeIcon>
  </Indicator>
)
