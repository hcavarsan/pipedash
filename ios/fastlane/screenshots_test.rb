require "minitest/autorun"
require "minitest/mock"
require "fastlane"
require "deliver"
require "fileutils"
require "tmpdir"

Fastlane.load_actions

class ScreenshotLanesTest < Minitest::Test
  FIXTURE_ROOT = Dir.mktmpdir("pipedash-screenshot-lanes-")
  FIXTURE_FASTLANE = File.join(FIXTURE_ROOT, "ios/fastlane")
  FileUtils.mkdir_p([FIXTURE_FASTLANE, File.join(FIXTURE_ROOT, "ios/app-store-screenshots")])
  FileUtils.cp(File.join(__dir__, "Fastfile"), FIXTURE_FASTLANE)
  %w[app_icon_appearances.rb required_reason_apis.rb ios_project.rb app_review_demo.rb review_notes.txt].each do |name|
    FileUtils.ln_s(File.join(__dir__, name), FIXTURE_FASTLANE)
  end
  FASTFILE = Fastlane::FastFile.new(File.join(FIXTURE_FASTLANE, "Fastfile"))
  Minitest.after_run { FileUtils.remove_entry(FIXTURE_ROOT) }

  def setup
    @output = File.join(FIXTURE_FASTLANE, "screenshots")
    FileUtils.mkdir_p(File.join(@output, "en-US"))
    %w[iphone ipad].each do |device|
      source = File.join(__dir__, "../app-store-screenshots/public/screenshots/apple", device, "en/01.png")
      FileUtils.cp(source, File.join(@output, "en-US/01-#{device}.png"))
    end
    @events = events = []
    @scenario = scenario = {}
    @file = FASTFILE
    @file.define_singleton_method(:sh) do |*command|
      raise scenario[:export_error] if command.last == "export:fastlane" && scenario[:export_error]
    end
    @file.define_singleton_method(:prepare_screenshots) do
      runner.lanes.fetch(:ios).fetch(:prepare_screenshots).call({})
    end
    @file.define_singleton_method(:asc_api_key) { events << :authenticate; {} }
    @file.define_singleton_method(:build_signed_ipa) { |**_| events << :sign; "test.ipa" }
    @file.define_singleton_method(:upload_to_app_store) do |**options|
      events << :app_store
      scenario[:published] = options
    end
    @file.define_singleton_method(:upload_to_testflight) { |**_| events << :testflight }
    @file.define_singleton_method(:github_api) do |**options|
      events << :github
      (scenario[:github_requests] ||= []) << options
      raise scenario[:github_error] if scenario[:github_error]

      path = options.fetch(:path)
      json = case path
             when "/repos/review/demo"
               { "private" => false, "permissions" => { "push" => true }, "default_branch" => "main" }
             when /\/contents\//
               { "content" => Base64.strict_encode64("on: workflow_dispatch") }
             when /\/actions\/workflows\?/
               { "workflows" => [{ "id" => 7, "name" => "Review demo", "state" => "active", "path" => ".github/workflows/demo.yml" }] }
             when /\/runs\?/
               { "workflow_runs" => [{ "id" => 42 }] }
             when /\/jobs\?/
               { "jobs" => [{ "id" => 1 }] }
             else
               raise "Unexpected GitHub path: #{path}"
             end
      { status: scenario.fetch(:github_status, 200), json: json, body: "demo-token-must-not-leak" }
    end
    @file.define_singleton_method(:method_missing) { |name, *_| raise "Unexpected action: #{name}" }
  end

  def teardown
    FileUtils.remove_entry(@output)
  end

  def run_lane(name, options = {})
    @file.runner.lanes.fetch(:ios).fetch(name).call(options)
  end

  def with_demo
    values = { "APP_REVIEW_DEMO_TOKEN" => "demo-token-must-not-leak", "APP_REVIEW_DEMO_REPOSITORY" => "review/demo" }
    previous = ENV.to_h.slice(*values.keys)
    ENV.update(values)
    yield
  ensure
    values.each_key { |key| previous.key?(key) ? ENV[key] = previous.fetch(key) : ENV.delete(key) }
  end

  def test_failed_export_blocks_both_publishing_lanes_before_apple_actions
    @scenario[:export_error] = "export failed"

    %i[release upload_screenshots].each do |lane|
      error = assert_raises(RuntimeError) { run_lane(lane) }
      assert_equal "export failed", error.message
      assert_empty @events
    end
  end

  def test_extra_locale_blocks_upload_without_deleting_existing_images
    foreign = File.join(@output, "fr-FR")
    FileUtils.mkdir_p(foreign)
    sentinel = File.join(foreign, "keep.png")
    File.binwrite(sentinel, "existing screenshot")

    %i[release upload_screenshots].each do |lane|
      error = assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(lane) }
      assert_includes error.message, "must contain only en-US"
      assert_empty @events
      assert_equal "existing screenshot", File.binread(sentinel)
    end
  end

  def test_screenshot_upload_cannot_publish_binary_metadata_version_or_review
    run_lane(:upload_screenshots)
    runner = Deliver::Runner.allocate
    runner.options = { ipa: "unrelated.ipa", app_version: "99.0.0" }.merge(@scenario.fetch(:published))
    %i[verify_version upload_binary submit_for_review].each do |operation|
      runner.define_singleton_method(operation) { raise "Unexpected publication: #{operation}" }
    end
    runner.define_singleton_method(:upload_metadata) do
      Deliver.stub(:cache, { app: Minitest::Mock.new }) do
        Deliver::UploadMetadata.new(options).upload
      end
    end
    runner.run

    screenshots = Deliver::UploadScreenshots.new.collect_screenshots(runner.options)
    assert_equal [
      ["en-US", "APP_IPAD_PRO_3GEN_129"],
      ["en-US", "APP_IPHONE_67"],
    ], screenshots.map { |shot| [shot.language, shot.display_type] }.sort
  end

  def test_beta_reaches_testflight_without_using_the_screenshot_exporter
    @scenario[:export_error] = "screenshot exporter unavailable"

    run_lane(:beta)

    assert_includes @events, :testflight
    refute_includes @events, :app_store
    assert_nil @scenario[:published]
  end

  def test_release_checks_demo_before_signing_and_interpolates_review_credentials
    with_demo { run_lane(:release) }

    requests = @scenario.fetch(:github_requests)
    assert_equal 5, requests.length
    requests.each do |request|
      assert_equal "GET", request[:http_method]
      assert_equal "https://api.github.com", request[:server_url]
      assert_equal "demo-token-must-not-leak", request[:api_bearer]
      assert_equal true, request[:secure]
      assert_nil request.fetch(:error_handlers).fetch("*").call({ body: "demo-token-must-not-leak" })
    end
    assert_equal [:github] * 5 + [:authenticate, :sign, :app_store], @events
    notes = @scenario.fetch(:published).fetch(:app_review_information).fetch(:notes)
    assert_includes notes, "demo-token-must-not-leak"
    assert_includes notes, "review/demo"
    assert_includes notes, "Review demo"
  end

  def test_invalid_demo_access_blocks_apple_actions_without_leaking_response_body
    @scenario[:github_status] = 403
    error = with_demo { assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:release) } }

    assert_equal [:github], @events
    assert_includes error.message, "HTTP 403"
    refute_includes error.message, "demo-token-must-not-leak"
  end

  def test_demo_network_failure_does_not_leak_exception_credentials
    @scenario[:github_error] = RuntimeError.new("demo-token-must-not-leak")
    error = with_demo { assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:release) } }

    assert_equal [:github], @events
    assert_includes error.message, "GitHub connectivity"
    refute_includes error.message, "demo-token-must-not-leak"
  end

  def test_verbose_release_stops_before_authenticated_requests
    FastlaneCore::Globals.stub(:verbose?, true) do
      error = assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:release) }
      assert_includes error.message, "without --verbose"
    end
    assert_empty @events
  end

  def test_packaged_extension_blocks_verification_before_release
    info = {
      "CFBundleDisplayName" => "Pipedash",
      "NSFaceIDUsageDescription" => "Use Face ID",
      "NSLocalNetworkUsageDescription" => "Connect to local providers",
      "UIFileSharingEnabled" => true,
      "LSSupportsOpeningDocumentsInPlace" => true,
      "ITSAppUsesNonExemptEncryption" => false,
      "CFBundleIcons" => {},
      "PipedashBuildWatchEnabled" => false,
      "BGTaskSchedulerPermittedIdentifiers" => ["com.henrique.pipedash.notifications-refresh"],
      "UIBackgroundModes" => ["fetch"],
    }
    @file.define_singleton_method(:plist_to_hash) { |_path| info }
    @file.define_singleton_method(:sh) do |*command|
      raise "Unexpected shell command" unless command.first == "unzip"

      FileUtils.mkdir_p(File.join(command.last, "Payload/Pipedash.app/PlugIns/BuildActivity.appex"))
    end

    error = assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:verify_ipa, ipa: "fixture.ipa") }
    assert_includes error.message, "must not bundle app extensions"
    assert_empty @events
  end
end
