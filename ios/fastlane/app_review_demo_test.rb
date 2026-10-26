require "minitest/autorun"
require_relative "app_review_demo"

class AppReviewDemoTest < Minitest::Test
  def setup
    @repository = "review/demo"
    @repo = { "private" => false, "archived" => false, "disabled" => false, "permissions" => { "push" => true }, "default_branch" => "main" }
    @workflow = { "id" => 7, "name" => "Review demo", "path" => ".github/workflows/demo.yml", "state" => "active" }
    @source = { "content" => Base64.strict_encode64("on:\n  workflow_dispatch:\njobs: {}\n") }
    @responses = {
      "/repos/review/demo" => @repo,
      "/repos/review/demo/actions/workflows?per_page=100" => { "workflows" => [@workflow] },
      "/repos/review/demo/contents/.github/workflows/demo.yml?ref=main" => @source,
      "/repos/review/demo/actions/workflows/7/runs?branch=main&event=workflow_dispatch&status=success&per_page=1" => { "workflow_runs" => [{ "id" => 42 }] },
      "/repos/review/demo/actions/runs/42/jobs?per_page=1" => { "jobs" => [{ "id" => 1 }] },
    }
    @requests = []
  end

  def verify
    AppReviewDemo.verify!(repository: @repository) do |path|
      @requests << path
      { status: 200, json: @responses.fetch(path) }
    end
  end

  def test_checks_default_branch_dispatch_success_and_jobs_without_mutating_github
    assert_equal "Review demo", verify
    assert_equal @responses.keys, @requests
  end

  def test_rejects_invalid_repository_before_requesting_github
    ["", "https://github.com/review/demo", "review/demo/extra", "review/..", "review/demo?token=secret"].each do |repository|
      @repository = repository
      assert_raises(AppReviewDemo::Error) { verify }
    end
    assert_empty @requests
  end

  def test_http_failure_does_not_expose_response_body_or_token
    [401, 403, 404, 429, 500].each do |status|
      error = assert_raises(AppReviewDemo::Error) do
        AppReviewDemo.verify!(repository: @repository) { { status: status, body: "private-token", json: { "message" => "private-token" } } }
      end
      assert_includes error.message, "HTTP #{status}"
      refute_includes error.message, "private-token"
    end
  end

  def test_requires_public_active_repository_with_write_access
    [{ "private" => true }, { "archived" => true }, { "disabled" => true }, { "permissions" => { "push" => false } }].each do |change|
      original = @repo.dup
      @repo.merge!(change)
      assert_raises(AppReviewDemo::Error) { verify }
      @repo.replace(original)
    end
  end

  def test_accepts_github_dispatch_event_yaml_forms
    ["on: workflow_dispatch", "on: [push, workflow_dispatch]", "'on':\n  workflow_dispatch: {}"].each do |definition|
      @source["content"] = Base64.strict_encode64(definition)
      assert_equal "Review demo", verify
    end
  end

  def test_requires_defaults_for_required_workflow_inputs
    @source["content"] = Base64.strict_encode64("on:\n  workflow_dispatch:\n    inputs:\n      environment:\n        required: true\n")
    assert_raises(AppReviewDemo::Error) { verify }
    @source["content"] = Base64.strict_encode64("on:\n  workflow_dispatch:\n    inputs:\n      environment:\n        required: true\n        default: demo\n")
    assert_equal "Review demo", verify
  end

  def test_rejects_disabled_and_non_dispatchable_workflows
    @workflow["state"] = "disabled_manually"
    assert_raises(AppReviewDemo::Error) { verify }
    @workflow["state"] = "active"
    @source["content"] = Base64.strict_encode64("on: push")
    assert_raises(AppReviewDemo::Error) { verify }
  end

  def test_requires_successful_manual_run_and_available_jobs
    runs = @responses.values.find { |value| value.key?("workflow_runs") }
    runs["workflow_runs"] = []
    assert_raises(AppReviewDemo::Error) { verify }
    runs["workflow_runs"] = [{ "id" => 42 }]
    @responses.fetch("/repos/review/demo/actions/runs/42/jobs?per_page=1")["jobs"] = []
    assert_raises(AppReviewDemo::Error) { verify }
  end

  def test_encodes_default_branch_in_content_and_run_queries
    @repo["default_branch"] = "review/demo&safe"
    @responses["/repos/review/demo/contents/.github/workflows/demo.yml?ref=review%2Fdemo%26safe"] = @source
    @responses["/repos/review/demo/actions/workflows/7/runs?branch=review%2Fdemo%26safe&event=workflow_dispatch&status=success&per_page=1"] = { "workflow_runs" => [{ "id" => 42 }] }
    assert_equal "Review demo", verify
  end
end
