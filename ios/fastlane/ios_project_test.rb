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

  def test_configuration_preserves_app_build_and_embeds_only_the_notification_extension
    project = spec
    original = Marshal.load(Marshal.dump(project.fetch("targets").fetch(IosProject::APP_TARGET)))
    configure(project)
    app = project.fetch("targets").fetch(IosProject::APP_TARGET)
    assert_equal original["preBuildScripts"], app["preBuildScripts"]
    assert_equal original["settings"]["groups"], app["settings"]["groups"]
    assert_equal original["dependencies"] + [{ "target" => IosProject::NSE_TARGET, "embed" => true }], app["dependencies"]
    assert_equal original["info"]["properties"]["LSRequiresIPhoneOS"], app["info"]["properties"]["LSRequiresIPhoneOS"]
    assert_equal [IosProject::APP_TARGET, IosProject::NSE_TARGET], project.fetch("targets").keys
    refute project.key?("packages")
  end

  def test_extension_target_is_a_notification_service_with_the_shared_keychain_group
    extension = configure.fetch("targets").fetch(IosProject::NSE_TARGET)
    assert_equal "com.henrique.pipedash.NotificationService", IosProject::NSE_ID
    assert_equal "app-extension", extension["type"]
    assert_equal "15.0", extension["deploymentTarget"]
    assert_equal IosProject::NSE_ID, extension.dig("settings", "base", "PRODUCT_BUNDLE_IDENTIFIER")
    assert_equal "PipedashNotificationService/NotificationService.entitlements", extension.dig("entitlements", "path")
    properties = extension.dig("info", "properties")
    assert_equal "com.apple.usernotifications.service", properties.dig("NSExtension", "NSExtensionPointIdentifier")
    assert_equal "$(PRODUCT_MODULE_NAME).NotificationService", properties.dig("NSExtension", "NSExtensionPrincipalClass")
    assert_equal "$(AppIdentifierPrefix)com.henrique.pipedash", properties["PipedashKeychainGroup"]
    assert_equal true, properties.dig("NSAppTransportSecurity", "NSAllowsLocalNetworking")
    assert_equal "1.4.0", properties["CFBundleShortVersionString"]
    assert_equal "42", properties["CFBundleVersion"]
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
    extension = project.fetch("targets").fetch(IosProject::NSE_TARGET)
    assert_equal "2.5.6", extension.dig("info", "properties", "CFBundleShortVersionString")
    assert_equal "99", extension.dig("info", "properties", "CFBundleVersion")
    assert_equal "2.5.6", extension.dig("settings", "base", "MARKETING_VERSION")
    assert_equal "99", extension.dig("settings", "base", "CURRENT_PROJECT_VERSION")
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
      assert_equal [IosProject::APP_TARGET, IosProject::NSE_TARGET], project["targets"].keys
      assert_equal 1, project["targets"][IosProject::APP_TARGET]["dependencies"].count { |item| item["target"] == IosProject::NSE_TARGET }
      assert_equal "7", project["targets"][IosProject::APP_TARGET]["info"]["properties"]["CFBundleVersion"]
    end
  end

  def test_export_requires_manual_profiles_for_the_app_and_the_extension
    options = IosProject.export_options("match AppStore app", "match AppStore ext", team_id: "TEAM")
    assert_equal({ IosProject::APP_ID => "match AppStore app", IosProject::NSE_ID => "match AppStore ext" },
                 options[:provisioningProfiles])
    assert_equal "manual", options[:signingStyle]
    assert_equal "TEAM", options[:teamID]
    assert_equal "app-store-connect", options[:method]
    assert_raises(ArgumentError) { IosProject.export_options(nil, "match AppStore ext", team_id: "TEAM") }
    assert_raises(ArgumentError) { IosProject.export_options("match AppStore app", "", team_id: "TEAM") }
  end

  def packaged_app
    {
      "PipedashBuildWatchEnabled" => false,
      "PipedashRemotePushEnabled" => true,
      "BGTaskSchedulerPermittedIdentifiers" => [IosProject::NOTIFICATION_TASK_ID],
      "UIBackgroundModes" => ["fetch"],
    }
  end

  def test_packaging_accepts_disabled_watch_with_remote_push_and_notification_refresh
    IosProject.verify_metadata!(packaged_app)
  end

  def test_packaging_rejects_missing_or_reactivated_watch_flag
    [nil, true, "false", 0].each do |value|
      assert_raises(ArgumentError) { IosProject.verify_metadata!(packaged_app.merge("PipedashBuildWatchEnabled" => value)) }
    end
  end

  def test_packaging_requires_remote_push_to_be_enabled
    [nil, false, "true", 1].each do |value|
      error = assert_raises(ArgumentError) { IosProject.verify_metadata!(packaged_app.merge("PipedashRemotePushEnabled" => value)) }
      assert_includes error.message, "Remote push must be enabled"
    end
  end

  def test_packaging_requires_each_notification_refresh_declaration
    %w[BGTaskSchedulerPermittedIdentifiers UIBackgroundModes].each do |key|
      invalid = packaged_app
      invalid.delete(key)
      assert_raises(ArgumentError) { IosProject.verify_metadata!(invalid) }
    end
  end

  def test_packaging_rejects_live_activities_and_unused_background_capabilities
    [
      { "NSSupportsLiveActivities" => true },
      { "NSSupportsLiveActivitiesFrequentUpdates" => true },
      { "BGTaskSchedulerPermittedIdentifiers" => [IosProject::NOTIFICATION_TASK_ID, "com.henrique.pipedash.build-watch"] },
      { "UIBackgroundModes" => %w[fetch processing] },
      { "UIBackgroundModes" => %w[fetch remote-notification] },
    ].each do |capability|
      assert_raises(ArgumentError) { IosProject.verify_metadata!(packaged_app.merge(capability)) }
    end
  end

  def app_entitlements
    { "aps-environment" => "production", "keychain-access-groups" => ["TEAM.com.henrique.pipedash"] }
  end

  def test_packaging_requires_the_production_apns_environment
    IosProject.verify_entitlements!(app_entitlements)
    ["development", nil].each do |value|
      invalid = app_entitlements.merge("aps-environment" => value)
      assert_raises(ArgumentError) { IosProject.verify_entitlements!(invalid) }
    end
    assert_raises(ArgumentError) { IosProject.verify_entitlements!({ "keychain-access-groups" => ["TEAM.com.henrique.pipedash"] }) }
  end

  def test_app_and_extension_share_exactly_the_app_keychain_group
    [
      [],
      ["TEAM.com.henrique.other"],
      ["TEAM.com.henrique.pipedash", "TEAM.com.henrique.pipedash.extra"],
      ["TEAM.com.henrique.pipedash", "TEAM.com.apple.token"],
    ].each do |groups|
      assert_raises(ArgumentError) { IosProject.verify_entitlements!(app_entitlements.merge("keychain-access-groups" => groups)) }
      assert_raises(ArgumentError) { IosProject.verify_extension_entitlements!("keychain-access-groups" => groups) }
    end
    assert_raises(ArgumentError) { IosProject.verify_entitlements!("aps-environment" => "production") }
    assert_raises(ArgumentError) { IosProject.verify_extension_entitlements!({}) }
    IosProject.verify_extension_entitlements!("keychain-access-groups" => ["TEAM.com.henrique.pipedash"])
  end

  def test_extension_must_not_carry_an_apns_entitlement
    invalid = { "aps-environment" => "production", "keychain-access-groups" => ["TEAM.com.henrique.pipedash"] }
    assert_raises(ArgumentError) { IosProject.verify_extension_entitlements!(invalid) }
  end

  def extension_info
    {
      "CFBundleIdentifier" => IosProject::NSE_ID,
      "CFBundleShortVersionString" => "1.4.0",
      "CFBundleVersion" => "42",
      "NSExtension" => { "NSExtensionPointIdentifier" => "com.apple.usernotifications.service" },
    }
  end

  def test_extension_checks_bundle_id_point_and_versions
    app_info = { "CFBundleShortVersionString" => "1.4.0", "CFBundleVersion" => "42" }
    IosProject.verify_extension!(extension_info, app_info)
    [
      { "CFBundleIdentifier" => "com.henrique.pipedash.Other" },
      { "NSExtension" => { "NSExtensionPointIdentifier" => "com.apple.widgetkit-extension" } },
      { "NSExtension" => nil },
      { "CFBundleShortVersionString" => "1.4.1" },
      { "CFBundleVersion" => "43" },
    ].each do |change|
      assert_raises(ArgumentError) { IosProject.verify_extension!(extension_info.merge(change), app_info) }
    end
  end
end
