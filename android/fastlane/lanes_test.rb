require "minitest/autorun"
require "fastlane"
require "fileutils"
require "tmpdir"

Fastlane.load_actions

class AndroidLanesTest < Minitest::Test
  FIXTURE_ROOT = Dir.mktmpdir("pipedash-android-lanes-")
  FIXTURE_FASTLANE = File.join(FIXTURE_ROOT, "android/fastlane")
  FIXTURE_IOS = File.join(FIXTURE_ROOT, "ios/fastlane")
  FileUtils.mkdir_p([FIXTURE_FASTLANE, FIXTURE_IOS, File.join(FIXTURE_ROOT, "ios/app-store-screenshots"),
                     File.join(FIXTURE_ROOT, "crates/pipedash-desktop")])
  FileUtils.cp(File.join(__dir__, "Fastfile"), FIXTURE_FASTLANE)
  %w[android_project.rb metadata].each { |name| FileUtils.ln_s(File.join(__dir__, name), FIXTURE_FASTLANE) }
  %w[store_text.rb store_version.rb].each do |name|
    FileUtils.ln_s(File.expand_path("../../ios/fastlane/#{name}", __dir__), FIXTURE_IOS)
  end
  File.write(File.join(FIXTURE_ROOT, "crates/pipedash-desktop/tauri.conf.json"), '{"version":"0.9.0"}')
  FASTFILE = Dir.chdir(FIXTURE_FASTLANE) { Fastlane::FastFile.new(File.join(FIXTURE_FASTLANE, "Fastfile")) }
  Minitest.after_run { FileUtils.remove_entry(FIXTURE_ROOT) }

  SKIPS = %i[skip_upload_metadata skip_upload_changelogs skip_upload_images skip_upload_screenshots].freeze

  def setup
    @images = File.join(FIXTURE_FASTLANE, "screenshots/en-US/images")
    FileUtils.mkdir_p(File.join(@images, "phoneScreenshots"))
    File.write(File.join(@images, "phoneScreenshots/01.png"), "shot")
    File.write(File.join(@images, "featureGraphic.png"), "graphic")
    @previous_key = ENV["PLAY_SERVICE_ACCOUNT_JSON"]
    ENV["PLAY_SERVICE_ACCOUNT_JSON"] = "{}"
    @events = events = []
    @scenario = scenario = {}
    @file = FASTFILE
    @file.define_singleton_method(:sh) do |*command|
      raise scenario[:export_error] if command.last == "export:play" && scenario[:export_error]
    end
    @file.define_singleton_method(:prepare_screenshots) do
      runner.lanes.fetch(:android).fetch(:prepare_screenshots).call({})
    end
    @file.define_singleton_method(:build_signed_aab) do |version_code:|
      events << [:build, version_code]
      "test.aab"
    end
    @file.define_singleton_method(:google_play_track_version_codes) do |track:, **_|
      scenario.fetch(:codes, {}).fetch(track, [])
    end
    @file.define_singleton_method(:upload_to_play_store) do |**options|
      events << [:upload, options[:release_status]].compact
      if scenario[:draft_app] && options[:release_status] == "completed"
        FastlaneCore::UI.user_error!("Google Api Error: Invalid request - Only releases with status draft may be created on draft app.")
      end
      scenario[:published] = options
      path = options[:metadata_path]
      scenario[:metadata_files] = Dir.glob("**/*", base: path).sort if path
      scenario[:changelog] = Dir.glob("en-US/changelogs/*.txt", base: path).to_h { |f| [f, File.read(File.join(path, f))] } if path
    end
    @file.define_singleton_method(:github_api) do |**_options|
      events << :github
      return { status: 404, json: {} } if scenario[:release_missing]

      { status: 200, json: { "body" => scenario.fetch(:release_body) } }
    end
    @file.define_singleton_method(:method_missing) { |name, *_| raise "Unexpected action: #{name}" }
  end

  def teardown
    FileUtils.remove_entry(File.join(FIXTURE_FASTLANE, "screenshots"))
    @previous_key ? ENV["PLAY_SERVICE_ACCOUNT_JSON"] = @previous_key : ENV.delete("PLAY_SERVICE_ACCOUNT_JSON")
  end

  def run_lane(name, options = {})
    @file.runner.lanes.fetch(:android).fetch(name).call(options)
  end

  def test_beta_builds_the_next_version_code_and_uploads_only_the_bundle_to_closed_testing
    @scenario[:codes] = { "internal" => [1010, 2_026_010_520], "production" => [1015] }

    run_lane(:beta)

    assert_equal [[:build, 1016], [:upload, "completed"]], @events
    published = @scenario.fetch(:published)
    assert_equal "alpha", published.fetch(:track)
    assert_equal "test.aab", published.fetch(:aab)
    SKIPS.each { |skip| assert_equal true, published.fetch(skip) }
  end

  def test_beta_saves_a_draft_while_google_play_treats_the_app_as_a_draft
    @scenario[:draft_app] = true

    run_lane(:beta)

    assert_equal [[:build, 1006], [:upload, "completed"], [:upload, "draft"]], @events
    assert_equal "draft", @scenario.fetch(:published).fetch(:release_status)
  end

  def test_beta_does_not_hide_other_play_errors
    @file.define_singleton_method(:upload_to_play_store) { |**_| FastlaneCore::UI.user_error!("Google Api Error: forbidden") }

    error = assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:beta) }
    assert_includes error.message, "forbidden"
  end

  def test_failed_export_blocks_both_publishing_lanes
    @scenario[:export_error] = "export failed"

    %i[release upload_screenshots].each do |lane|
      error = assert_raises(RuntimeError) { run_lane(lane) }
      assert_equal "export failed", error.message
      assert_empty @events
    end
  end

  def test_missing_github_release_stops_before_building
    @scenario[:release_missing] = true

    error = assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:release) }

    assert_includes error.message, "No published GitHub release v0.9.0"
    assert_equal [:github], @events
  end

  def test_release_clips_whats_new_and_publishes_to_production
    @scenario[:codes] = { "alpha" => [1011] }
    @scenario[:release_body] = "### Added\n\n#{(1..40).map { |n| "- Item number #{n} with some words" }.join("\n")}\n"

    run_lane(:release)

    assert_equal [:github, [:build, 1012], [:upload, "completed"]], @events
    published = @scenario.fetch(:published)
    assert_equal "production", published.fetch(:track)
    assert_equal "completed", published.fetch(:release_status)
    changelog = @scenario.fetch(:changelog).fetch("en-US/changelogs/1012.txt")
    assert_operator changelog.length, :<=, 500
    assert changelog.end_with?("\n…")
    assert_includes @scenario.fetch(:metadata_files), "en-US/images/icon.png"
    assert_includes @scenario.fetch(:metadata_files), "en-US/images/featureGraphic.png"
    assert_includes @scenario.fetch(:metadata_files), "en-US/images/phoneScreenshots/01.png"
  end

  def test_screenshot_upload_never_builds_or_uploads_a_bundle
    error = assert_raises(FastlaneCore::Interface::FastlaneError) { run_lane(:upload_screenshots) }
    assert_includes error.message, "closed testing track has none"
    assert_empty @events

    @scenario[:codes] = { "alpha" => [1006, 1009], "internal" => [2_026_010_520] }
    run_lane(:upload_screenshots)

    assert_equal [[:upload]], @events
    published = @scenario.fetch(:published)
    assert_equal "alpha", published.fetch(:track)
    assert_equal 1009, published.fetch(:version_code)
    assert_equal true, published.fetch(:skip_upload_aab)
    assert_equal true, published.fetch(:skip_upload_apk)
    assert_equal false, published.fetch(:skip_upload_images)
    assert_empty @scenario.fetch(:changelog)
    assert_includes @scenario.fetch(:metadata_files), "en-US/images/phoneScreenshots/01.png"
  end
end
