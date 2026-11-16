import { Select } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { ALL_APPLICATIONS_QUERY } from '../lib/allApplicationsQuery'
import { useApplications, useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'

export function MergeModal({ detail, copy, onClose }: ApplicationModalProps): ReactNode {
  const [other, setOther] = useState<string | null>(null)
  const applications = useApplications(ALL_APPLICATIONS_QUERY)
  const edit = useEditApplication()
  const options = (applications.data?.applications ?? [])
    .filter((application) => application.id !== detail.summary.id)
    .map((application) => ({ value: String(application.id), label: application.name }))
  const save = (): void => {
    if (other !== null) {
      edit.mutate(
        { id: detail.summary.id, edit: { type: 'merge', other_application_id: Number(other) } },
        { onSuccess: onClose }
      )
    }
  }

  return (
    <StandardModal
      opened
      onClose={onClose}
      size="md"
      title={copy.merge}
      loading={applications.isPending}
      primaryAction={{ label: copy.merge, loading: edit.isPending, disabled: other === null, onClick: save }}
    >
      <Select label={copy.merge} data={options} value={other} onChange={setOther} searchable />
    </StandardModal>
  )
}
