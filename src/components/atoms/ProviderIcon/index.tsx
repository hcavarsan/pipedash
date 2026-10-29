import { Box, Image, Text, ThemeIcon } from '@mantine/core'
import { IconInfoCircleFilled } from '@tabler/icons-react'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { withBasePath } from '../../../lib/basePath'

interface ProviderIconProps {
  name: string
  icon: string | null
  size: number
  badge?: { color: string; label: string } | undefined
}

const initials = (name: string) => {
  const [first = '?', second] = name.match(/[\p{L}\p{N}]+/gu) ?? []

  return (second ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2)).toUpperCase()
}

export const ProviderIcon = ({ name, icon, size, badge }: ProviderIconProps) => {
  const mobileShell = useMobileShell()
  const fontSize = size <= 24 ? 'xs' : size <= 32 ? 'sm' : size <= 40 ? 'md' : size <= 48 ? 'lg' : 'xl'

  return (
    <Box
      pos="relative"
      display="inline-flex"
      style={{ flexShrink: 0 }}
      role={badge ? 'img' : undefined}
      aria-label={badge ? `${name}: ${badge.label}` : undefined}
    >
      <ThemeIcon size={size} variant="transparent" color="gray" aria-hidden>
        {icon ? (
          <Image src={withBasePath(icon)} alt="" w={size * 0.65} h={size * 0.65} fit="contain" />
        ) : (
          <Text fz={mobileShell ? size * 0.4 : fontSize} fw={700} lh={1}>
            {initials(name)}
          </Text>
        )}
      </ThemeIcon>
      {badge && (
        <IconInfoCircleFilled
          size={Math.max(10, Math.round(size * 0.45))}
          color={badge.color}
          aria-hidden
          style={{
            position: 'absolute',
            top: -3,
            right: -3,
            borderRadius: '50%',
            background: 'var(--mantine-color-body)',
          }}
        />
      )}
    </Box>
  )
}
