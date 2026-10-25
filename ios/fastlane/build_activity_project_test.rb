require "minitest/autorun"
require "tmpdir"
require_relative "build_activity_project"

class BuildActivityProjectTest < Minitest::Test
  def spec
    {
      "name" => "pipedash-desktop",
      "configs" => { "debug" => "debug", "release" => "release" },
      "targets" => {
        BuildActivityProject::APP_TARGET => {
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
    BuildActivityProject.configure(project, root: "/repo", project_dir: "/repo/crates/pipedash-desktop/gen/apple",
                                   version: version, build_number: build_number)
  end

  def test_existing_app_build_configuration_survives_extension_generation
    project = spec
    original = Marshal.load(Marshal.dump(project.fetch("targets").fetch(BuildActivityProject::APP_TARGET)))
    configure(project)
    app = project.fetch("targets").fetch(BuildActivityProject::APP_TARGET)
    assert_equal original["preBuildScripts"], app["preBuildScripts"]
    assert_equal original["settings"]["groups"], app["settings"]["groups"]
    original["dependencies"].each { |dependency| assert_includes app["dependencies"], dependency }
    assert_equal original["info"]["properties"]["LSRequiresIPhoneOS"], app["info"]["properties"]["LSRequiresIPhoneOS"]
  end

  def test_configure_is_idempotent
    project = configure
    first = YAML.dump(project)
    assert_equal first, YAML.dump(configure(project))
    embedded = project["targets"][BuildActivityProject::APP_TARGET]["dependencies"].count { |dependency| dependency["target"] == BuildActivityProject::TARGET }
    assert_equal 1, embedded
  end

  def test_versions_are_updated_together_on_every_preparation
    project = configure
    configure(project, version: "2.5.6", build_number: 99)
    project.fetch("targets").each_value do |target|
      assert_equal "2.5.6", target.dig("info", "properties", "CFBundleShortVersionString")
      assert_equal "99", target.dig("info", "properties", "CFBundleVersion")
      assert_equal "2.5.6", target.dig("settings", "base", "MARKETING_VERSION")
      assert_equal "99", target.dig("settings", "base", "CURRENT_PROJECT_VERSION")
    end
  end

  def test_existing_local_package_key_is_reused
    project = spec
    project["packages"] = { "ExistingNative" => { "path" => "/repo/crates/tauri-plugin-pipedash-native/ios" } }
    configure(project)
    assert_equal ["ExistingNative"], project["packages"].keys
    assert_equal "ExistingNative", project["targets"][BuildActivityProject::TARGET]["dependencies"].first["package"]
  end

  def test_conflicting_package_identity_is_rejected
    project = spec
    project["packages"] = { BuildActivityProject::PACKAGE => { "path" => "/another/native" } }
    assert_raises(ArgumentError) { configure(project) }
  end

  def test_invalid_versions_and_build_numbers_are_rejected
    ["1.4", "1.4.0-beta", "$(MARKETING_VERSION)"].each do |version|
      assert_raises(ArgumentError) { configure(version: version) }
    end
    [0, -1, "1.0", ""].each do |number|
      assert_raises(ArgumentError) { configure(build_number: number) }
    end
  end

  def test_write_round_trips_a_generated_spec_and_updates_existing_versions
    Dir.mktmpdir do |root|
      path = File.join(root, "project.yml")
      File.write(path, YAML.dump(spec))
      2.times { BuildActivityProject.write(path, root: root, version: "1.4.0", build_number: 7) }
      project = YAML.safe_load_file(path, aliases: true)
      assert_equal [BuildActivityProject::APP_TARGET, BuildActivityProject::TARGET], project["targets"].keys
      assert_equal "7", project["targets"][BuildActivityProject::TARGET]["info"]["properties"]["CFBundleVersion"]
    end
  end


  def test_missing_tauri_metadata_is_rejected
    assert_raises(ArgumentError) { BuildActivityProject.stage_tauri_api({ "packages" => [] }, root: "/repo") }
  end

  def test_export_options_include_a_separate_manual_profile_for_every_bundle
    profiles = { BuildActivityProject::APP_ID => "match AppStore app", BuildActivityProject::BUNDLE_ID => "match AppStore widget" }
    options = BuildActivityProject.export_options(profiles, team_id: "TEAM")
    assert_equal profiles, options[:provisioningProfiles]
    assert_equal "manual", options[:signingStyle]
    assert_equal "TEAM", options[:teamID]
    assert_equal "app-store-connect", options[:method]
    assert_raises(ArgumentError) do
      BuildActivityProject.export_options({ BuildActivityProject::APP_ID => "main only" }, team_id: "TEAM")
    end
  end

  def app_info
    {
      "NSSupportsLiveActivities" => true,
      "BGTaskSchedulerPermittedIdentifiers" => [BuildActivityProject::TASK_ID],
      "UIBackgroundModes" => ["processing"],
      "CFBundleShortVersionString" => "1.4.0",
      "CFBundleVersion" => "42",
    }
  end

  def extension_info
    {
      "CFBundleIdentifier" => BuildActivityProject::BUNDLE_ID,
      "NSExtension" => { "NSExtensionPointIdentifier" => "com.apple.widgetkit-extension" },
      "CFBundleShortVersionString" => "1.4.0",
      "CFBundleVersion" => "42",
    }
  end

  def test_packaging_rejects_missing_background_configuration
    BuildActivityProject.verify_metadata!(app_info, extension_info)
    %w[NSSupportsLiveActivities BGTaskSchedulerPermittedIdentifiers UIBackgroundModes].each do |key|
      app = app_info
      app.delete(key)
      assert_raises(ArgumentError) { BuildActivityProject.verify_metadata!(app, extension_info) }
    end
  end

  def test_packaging_rejects_extension_identity_type_and_version_mismatches
    %w[CFBundleIdentifier CFBundleShortVersionString CFBundleVersion NSExtension].each do |key|
      extension = extension_info
      extension[key] = key == "NSExtension" ? {} : "wrong"
      assert_raises(ArgumentError) { BuildActivityProject.verify_metadata!(app_info, extension) }
    end
  end

end
