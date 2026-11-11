import { Button } from '@mantine/core'
import { IconArrowLeft, IconChevronLeft } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { isRecord } from '../../../utils/isRecord'
import { Onboarding } from '../../setup/components/SetupWizard/Onboarding'
import { ConnectServerForm } from './ConnectServerForm'
import { EntryScreen } from './EntryScreen'

const TITLE = 'Connect to a server'
const ALERTS_TITLE = 'Get alerts from a server'

export function ConnectServerScreen(): ReactNode {
  const navigate = useNavigate()
  const location = useLocation()
  const mobileShell = useMobileShell()
  const alerts = useSearchParams()[0].get('purpose') === 'alerts'
  const title = alerts ? ALERTS_TITLE : TITLE

  const goBack = (): void => {
    if (location.key !== 'default') {
      void navigate(-1)
    } else {
      void navigate('/')
    }
  }

  const link = isRecord(location.state) && typeof location.state.link === 'string' ? location.state.link : ''
  const form = <ConnectServerForm key={link} mobileShell={mobileShell} link={link} alerts={alerts} />

  if (mobileShell) {
    return (
      <Onboarding
        title={title}
        footer={
          <Button
            fullWidth
            size="lg"
            variant="subtle"
            leftSection={<IconChevronLeft size={16} aria-hidden />}
            onClick={goBack}
          >
            Back
          </Button>
        }
      >
        <DocumentTitle title={title} />
        {form}
      </Onboarding>
    )
  }

  return (
    <EntryScreen title={title} documentTitle={title}>
      {form}

      <Button
        variant="subtle"
        color="gray"
        leftSection={<IconArrowLeft size={16} aria-hidden />}
        onClick={goBack}
        style={{ alignSelf: 'flex-start' }}
      >
        Back
      </Button>
    </EntryScreen>
  )
}
