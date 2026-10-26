require "minitest/autorun"
require "tmpdir"
require_relative "ios_project"

class IosProjectTest < Minitest::Test
  def spec
    {
      "name" => "pipedash-desktop",
      "configs" => { "debug" => "debug", "release" => "release" },
      "targets" => {
        IosProject::APP_TARGET => {
          "type" => "application",
          "platform" => "iOS",
          "info" => { "path" => "pipedash-desktop_iOS/Info.plist", "properties" => { "LSRequiresIPhoneOS" => true } },
          "settings" => { "base" => { "ENABLE_BITCODE" => false }, "groups" => ["app"] },
          "dependencies" => [{ "framework" => "libapp.a", "embed" => false }],
          "preBuildScripts" => [{ "name" => "Build Rust Code", "script" => "tauri ios xcode-script" }],
        },
      },
    }
  end

  def configure(project = spec, version: "1.4.0", build_number: 42)
    IosProject.configure(project, version: version, build_number: build_number)
  end

  def test_configuration_preserves_app_build_and_adds_no_extension_or_package
    project = spec
    original = Marshal.load(Marshal.dump(project.fetch("targets").fetch(IosProject::APP_TARGET)))
    configure(project)
    app = project.fetch("targets").fetch(IosProject::APP_TARGET)
    assert_equal original["preBuildScripts"], app["preBuildScripts"]
    assert_equal original["settings"]["groups"], app["settings"]["groups"]
    assert_equal original["dependencies"], app["dependencies"]
    assert_equal original["info"]["properties"]["LSRequiresIPhoneOS"], app["info"]["properties"]["LSRequiresIPhoneOS"]
    assert_equal [IosProject::APP_TARGET], project.fetch("targets").keys
    refute project.key?("packages")
  end

  def test_configure_is_idempotent
    project = configure
    first = YAML.dump(project)
    assert_equal first, YAML.dump(configure(project))
  end

  def test_versions_are_updated_on_every_preparation
    project = configure
    configure(project, version: "2.5.6", build_number: 99)
    app = project.fetch("targets").fetch(IosProject::APP_TARGET)
    assert_equal "2.5.6", app.dig("info", "properties", "CFBundleShortVersionString")
    assert_equal "99", app.dig("info", "properties", "CFBundleVersion")
    assert_equal "2.5.6", app.dig("settings", "base", "MARKETING_VERSION")
    assert_equal "99", app.dig("settings", "base", "CURRENT_PROJECT_VERSION")
  end

  def test_invalid_versions_and_build_numbers_are_rejected
    ["1.4", "1.4.0-beta", "$(MARKETING_VERSION)"].each do |version|
      assert_raises(ArgumentError) { configure(version: version) }
    end
    [0, -1, "1.0", ""].each do |number|
      assert_raises(ArgumentError) { configure(build_number: number) }
    end
  end

  def test_write_round_trips_a_generated_spec
    Dir.mktmpdir do |root|
      path = File.join(root, "project.yml")
      File.write(path, YAML.dump(spec))
      2.times { IosProject.write(path, version: "1.4.0", build_number: 7) }
      project = YAML.safe_load_file(path, aliases: true)
      assert_equal [IosProject::APP_TARGET], project["targets"].keys
      assert_equal "7", project["targets"][IosProject::APP_TARGET]["info"]["properties"]["CFBundleVersion"]
    end
  end

  def test_export_requires_only_the_app_manual_profile
    options = IosProject.export_options("match AppStore app", team_id: "TEAM")
    assert_equal({ IosProject::APP_ID => "match AppStore app" }, options[:provisioningProfiles])
    assert_equal "manual", options[:signingStyle]
    assert_equal "TEAM", options[:teamID]
    assert_equal "app-store-connect", options[:method]
    assert_raises(ArgumentError) { IosProject.export_options(nil, team_id: "TEAM") }
  end

  def test_packaging_accepts_disabled_features_without_background_declarations
    IosProject.verify_metadata!("PipedashBuildWatchEnabled" => false)
  end

  def test_packaging_rejects_missing_or_reactivated_feature_flag
    [nil, true, "false", 0].each do |value|
      assert_raises(ArgumentError) { IosProject.verify_metadata!("PipedashBuildWatchEnabled" => value) }
    end
  end

  def test_packaging_rejects_live_activities_and_unused_background_capabilities
    [
      { "NSSupportsLiveActivities" => true },
      { "NSSupportsLiveActivitiesFrequentUpdates" => true },
      { "BGTaskSchedulerPermittedIdentifiers" => ["com.henrique.pipedash.build-watch"] },
      { "UIBackgroundModes" => ["processing"] },
      { "UIBackgroundModes" => ["remote-notification"] },
    ].each do |capability|
      app = { "PipedashBuildWatchEnabled" => false }.merge(capability)
      assert_raises(ArgumentError) { IosProject.verify_metadata!(app) }
    end
  end

  def test_packaging_rejects_apns_entitlement_without_restricting_local_notifications
    IosProject.verify_entitlements!({})
    IosProject.verify_entitlements!("application-identifier" => "TEAM.com.henrique.pipedash")
    assert_raises(ArgumentError) { IosProject.verify_entitlements!("aps-environment" => "production") }
    assert_raises(ArgumentError) { IosProject.verify_entitlements!("aps-environment" => "development") }
  end
end
