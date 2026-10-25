import { Image, Indicator, Text, ThemeIcon } from '@mantine/core'
import { IconAlertOctagon, IconAlertTriangle } from '@tabler/icons-react'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { withBasePath } from '../../../lib/basePath'
import type { AttentionTone } from '../../../utils/providerAttention'

interface ProviderIconProps {
  name: string
  icon: string | null
  size: number
  attention?: { tone: AttentionTone; label: string }
}

const TONE_COLOR: Record<AttentionTone, string> = {
  warning: 'orange',
  danger: 'red',
}

const initials = (name: string) => {
  const words = name.match(/[\p{L}\p{N}]+/gu) ?? ['?']

  return (words.length > 1 ? `${words[0][0]}${words[1][0]}` : words[0].slice(0, 2)).toUpperCase()
}

export const ProviderIcon = ({ name, icon, size, attention }: ProviderIconProps) => {
  const mobileShell = useMobileShell()
  const fontSize = size <= 24 ? 'xs' : size <= 32 ? 'sm' : size <= 40 ? 'md' : size <= 48 ? 'lg' : 'xl'
  const AttentionIcon = attention?.tone === 'danger' ? IconAlertOctagon : IconAlertTriangle

  return (
    <Indicator
      color={attention ? TONE_COLOR[attention.tone] : 'gray'}
      size={Math.max(mobileShell ? 8 : 16, size / 3)}
      offset={1}
      withBorder
      disabled={!attention}
      role={attention ? 'img' : undefined}
      aria-label={attention?.label}
      label={!mobileShell && attention ? <AttentionIcon size={14} aria-hidden /> : undefined}
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
    </Indicator>
  )
}
