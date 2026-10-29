DO $$
DECLARE
  argocd BIGINT := (SELECT id FROM providers WHERE name = 'argocd');
  fluxcd BIGINT := (SELECT id FROM providers WHERE name = 'fluxcd');
  gitea BIGINT := (SELECT id FROM providers WHERE name = 'gitea');
  jenkins BIGINT := (SELECT id FROM providers WHERE name = 'jenkins');
  komodo BIGINT := (SELECT id FROM providers WHERE name = 'komodo');
  pinned BIGINT := (SELECT id FROM pipeline_views WHERE kind = 'pinned');
  web_team BIGINT;
  build_test BIGINT;
  builds BIGINT;
  tests BIGINT;
  releases BIGINT;
  gitops BIGINT;
  flat_by_provider CONSTANT TEXT := '{"sort":"name","direction":"asc","sections":"provider","layout":"table"}';
  cards CONSTANT TEXT := '{"sort":"name","direction":"asc","sections":"none","layout":"cards"}';
BEGIN
  IF EXISTS (SELECT 1 FROM pipeline_views WHERE kind = 'custom') THEN
    RETURN;
  END IF;

  INSERT INTO pipeline_views (kind, name, position, rules_json, display_json)
  VALUES ('custom', 'Needs attention', 0, '{"statuses":["failed"]}',
          '{"sort":"last_run","direction":"desc","sections":"provider","layout":"table"}');

  INSERT INTO pipeline_views (kind, name, position, rules_json, display_json)
  VALUES ('custom', 'Web team', 1, '{"repositories":["acme/web"]}', flat_by_provider)
  RETURNING id INTO web_team;

  INSERT INTO pipeline_views (kind, name, position, display_json)
  VALUES ('custom', 'Build & test', 2, flat_by_provider)
  RETURNING id INTO build_test;

  INSERT INTO pipeline_views (kind, name, parent_id, position, rules_json, display_json)
  VALUES ('custom', 'Builds', build_test, 0, '{"name_contains":"build"}', flat_by_provider)
  RETURNING id INTO builds;

  INSERT INTO pipeline_views (kind, name, parent_id, position, rules_json, display_json)
  VALUES ('custom', 'Tests', build_test, 1, '{"name_contains":"test"}', flat_by_provider)
  RETURNING id INTO tests;

  INSERT INTO pipeline_views (kind, name, position, display_json)
  VALUES ('custom', 'Releases', 3, flat_by_provider)
  RETURNING id INTO releases;

  INSERT INTO pipeline_views (kind, name, parent_id, position, rules_json, display_json)
  VALUES ('custom', 'Deploy jobs', releases, 0, '{"name_contains":"deploy"}', flat_by_provider),
         ('custom', 'Mobile', releases, 1, '{"repositories":["acme/mobile"]}', flat_by_provider);

  INSERT INTO pipeline_views (kind, name, position, display_json)
  VALUES ('custom', 'GitOps', 4, cards)
  RETURNING id INTO gitops;

  INSERT INTO pipeline_views (kind, name, parent_id, position, rules_json, display_json)
  VALUES ('custom', 'Argo CD apps', gitops, 0,
          jsonb_build_object('provider_ids', jsonb_build_array(argocd))::TEXT, cards),
         ('custom', 'Flux Kustomizations', gitops, 1,
          jsonb_build_object('provider_ids', jsonb_build_array(fluxcd), 'name_contains', 'Kustomization')::TEXT, cards),
         ('custom', 'Flux Helm releases', gitops, 2,
          jsonb_build_object('provider_ids', jsonb_build_array(fluxcd), 'name_contains', 'HelmRelease')::TEXT, cards);

  INSERT INTO pipeline_views (kind, name, position, rules_json, display_json)
  VALUES ('custom', 'Containers', 5, jsonb_build_object('provider_ids', jsonb_build_array(komodo))::TEXT, cards);

  INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position)
  SELECT m.view_id, c.provider_id, c.id, m.position
  FROM (VALUES
    (web_team, argocd, NULL::TEXT, 'web', 0),
    (web_team, komodo, NULL::TEXT, 'storefront', 1),
    (web_team, jenkins, NULL::TEXT, 'build', 2),
    (builds, gitea, 'acme/web', 'CI', 0),
    (builds, gitea, 'acme/api', 'CI', 1),
    (tests, jenkins, NULL::TEXT, 'nightly-e2e', 0),
    (pinned, gitea, 'acme/mobile', 'Release', 0),
    (pinned, argocd, NULL::TEXT, 'web', 1),
    (pinned, komodo, NULL::TEXT, 'storefront', 2)
  ) AS m(view_id, provider_id, repository, name, position)
  JOIN pipelines_cache c
    ON c.provider_id = m.provider_id
   AND c.name = m.name
   AND (m.repository IS NULL OR c.repository = m.repository);
END $$;
