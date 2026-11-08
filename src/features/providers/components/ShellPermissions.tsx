import { Stack } from '@mantine/core'
import { IconCheck, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import classes from '../../../components/forms/ShellForm.module.css'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import type { FeatureAvailability, PermissionCheck, PermissionStatus } from '../../../types/plugins'
import { formatPermissionDescription } from './PermissionCard'

function ShellPermissionRow({
  check,
  features,
}: {
  check: PermissionCheck
  features: FeatureAvailability[]
}): ReactNode {
  const featureNames = features
    .filter((f) => f.feature.required_permissions.includes(check.permission.name))
    .map((f) => f.feature.name)

  return (
    <IosRow
      title={<span className={classes.permissionName}>{check.permission.name}</span>}
      subtitle={
        <>
          <span className={`${classes.line} ${check.granted ? classes.granted : classes.missing}`}>
            {check.granted ? <IconCheck size={14} aria-hidden /> : <IconX size={14} aria-hidden />}
            {check.granted ? 'Granted' : 'Missing'} · {check.permission.required ? 'Required' : 'Optional'}
          </span>
          <span className={classes.line}>{formatPermissionDescription(check.permission.description)}</span>
          {featureNames.length > 0 && (
            <span className={classes.line}>
              {check.granted ? 'With this you can: ' : "Missing this means you can't: "}
              {featureNames.join(', ')}
            </span>
          )}
        </>
      }
    />
  )
}

export function ShellPermissions({
  status,
  features,
}: {
  status: PermissionStatus
  features: FeatureAvailability[]
}): ReactNode {
  return (
    <Stack gap="lg">
      {status.credential_label && (
        <IosGroup>
          <IosRow title="Token type" value={status.credential_label} />
        </IosGroup>
      )}
      <IosGroup>
        {status.permissions.map((check) => (
          <ShellPermissionRow key={check.permission.name} check={check} features={features} />
        ))}
      </IosGroup>
    </Stack>
  )
}
