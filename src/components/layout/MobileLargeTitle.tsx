import { Title } from '@mantine/core'
import { useLocation } from 'react-router-dom'

import { mobileChrome } from './mobileChrome'

export function MobileLargeTitle() {
  const { pathname, search } = useLocation()
  const { title, back } = mobileChrome(pathname, search)

  if (back) {
    return null
  }

  return (
    <Title order={1} fz={34} fw={700} lh={1.1} px="xs" pt={2} pb="xs" data-page-heading tabIndex={-1}>
      {title}
    </Title>
  )
}
