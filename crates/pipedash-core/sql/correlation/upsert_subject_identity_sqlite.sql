INSERT INTO subject_identities (subject_id, provider_id, unit_id, unit_parent, unit_depends_on_json, app_ref_authority, app_ref_scope, app_ref_id, app_label)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (subject_id) DO UPDATE SET
    provider_id = excluded.provider_id,
    unit_id = excluded.unit_id,
    unit_parent = excluded.unit_parent,
    unit_depends_on_json = excluded.unit_depends_on_json,
    app_ref_authority = excluded.app_ref_authority,
    app_ref_scope = excluded.app_ref_scope,
    app_ref_id = excluded.app_ref_id,
    app_label = excluded.app_label
