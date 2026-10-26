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
  %w[app_icon_appearances.rb required_reason_apis.rb ios_project.rb].each do |name|
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
    @file.define_singleton_method(:method_missing) { |name, *_| raise "Unexpected action: #{name}" }
  end

  def teardown
    FileUtils.remove_entry(@output)
  end

  def run_lane(name)
    @file.runner.lanes.fetch(:ios).fetch(name).call({})
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
end
