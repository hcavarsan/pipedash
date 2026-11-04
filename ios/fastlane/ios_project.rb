require "yaml"

module IosProject
  APP_ID = "com.henrique.pipedash"
  APP_TARGET = "pipedash-desktop_iOS"
  NSE_TARGET = "PipedashNotificationService"
  NSE_ID = "#{APP_ID}.NotificationService"
  NSE_POINT = "com.apple.usernotifications.service"
  NOTIFICATION_TASK_ID = "#{APP_ID}.notifications-refresh"
  KEYCHAIN_GROUP = "$(AppIdentifierPrefix)#{APP_ID}"

  def self.configure(spec, version:, build_number:)
    raise ArgumentError, "Expected a three-part iOS version" unless version.match?(/\A\d+\.\d+\.\d+\z/)
    raise ArgumentError, "Expected a positive build number" unless build_number.to_s.match?(/\A[1-9]\d*\z/)

    app = spec.fetch("targets").fetch(APP_TARGET)
    settings = (app["settings"] ||= {})["base"] ||= {}
    settings.merge!("MARKETING_VERSION" => version, "CURRENT_PROJECT_VERSION" => build_number.to_s)
    properties = app.fetch("info")["properties"] ||= {}
    properties.merge!("CFBundleShortVersionString" => version, "CFBundleVersion" => build_number.to_s)
    dependencies = app["dependencies"] ||= []
    dependencies << { "target" => NSE_TARGET, "embed" => true } unless dependencies.any? { |item| item["target"] == NSE_TARGET }
    spec["targets"][NSE_TARGET] = extension_target(version, build_number.to_s)
    spec
  end

  def self.extension_target(version, build_number)
    {
      "type" => "app-extension",
      "platform" => "iOS",
      "deploymentTarget" => "15.0",
      "sources" => [{ "path" => NSE_TARGET }],
      "entitlements" => { "path" => "#{NSE_TARGET}/NotificationService.entitlements" },
      "info" => {
        "path" => "#{NSE_TARGET}/Info.plist",
        "properties" => {
          "CFBundleDisplayName" => "Pipedash",
          "CFBundleShortVersionString" => version,
          "CFBundleVersion" => build_number,
          "NSExtension" => {
            "NSExtensionPointIdentifier" => NSE_POINT,
            "NSExtensionPrincipalClass" => "$(PRODUCT_MODULE_NAME).NotificationService",
          },
          "PipedashKeychainGroup" => KEYCHAIN_GROUP,
          "NSAppTransportSecurity" => { "NSAllowsLocalNetworking" => true },
        },
      },
      "settings" => {
        "base" => {
          "PRODUCT_BUNDLE_IDENTIFIER" => NSE_ID,
          "SWIFT_VERSION" => "5.0",
          "MARKETING_VERSION" => version,
          "CURRENT_PROJECT_VERSION" => build_number,
        },
      },
    }
  end

  def self.write(path, version:, build_number:)
    spec = YAML.safe_load_file(path, aliases: true)
    configure(spec, version: version, build_number: build_number)
    File.write(path, YAML.dump(spec))
  end

  def self.export_options(app_profile, extension_profile, team_id:)
    { APP_ID => app_profile, NSE_ID => extension_profile }.each do |id, profile|
      raise ArgumentError, "Missing App Store profile for #{id}" if profile.to_s.empty?
    end

    {
      method: "app-store-connect",
      signingStyle: "manual",
      teamID: team_id,
      provisioningProfiles: { APP_ID => app_profile, NSE_ID => extension_profile },
    }
  end

  def self.verify_metadata!(app)
    raise ArgumentError, "Build watching must remain disabled for release" unless app["PipedashBuildWatchEnabled"] == false
    raise ArgumentError, "Remote push must be enabled for release" unless app["PipedashRemotePushEnabled"] == true
    if app["NSSupportsLiveActivities"] || app["NSSupportsLiveActivitiesFrequentUpdates"]
      raise ArgumentError, "Live Activities must remain disabled for release"
    end
    unless Array(app["BGTaskSchedulerPermittedIdentifiers"]) == [NOTIFICATION_TASK_ID]
      raise ArgumentError, "App must declare only the notifications background refresh task"
    end
    modes = Array(app["UIBackgroundModes"])
    raise ArgumentError, "App is missing background fetch mode" unless modes.include?("fetch")
    unless (modes & %w[processing remote-notification]).empty?
      raise ArgumentError, "Release must not declare background push or processing modes"
    end
  end

  def self.verify_keychain_group!(entitlements)
    groups = Array(entitlements["keychain-access-groups"])
    return if groups.length == 1 && groups.first.end_with?(".#{APP_ID}")

    raise ArgumentError, "App and extension must share only the app keychain group"
  end

  def self.verify_entitlements!(entitlements)
    unless entitlements["aps-environment"] == "production"
      raise ArgumentError, "Release must use the production APNs environment"
    end
    verify_keychain_group!(entitlements)
  end

  def self.verify_extension!(info, app_info)
    raise ArgumentError, "Extension must use bundle id #{NSE_ID}" unless info["CFBundleIdentifier"] == NSE_ID
    unless info.dig("NSExtension", "NSExtensionPointIdentifier") == NSE_POINT
      raise ArgumentError, "Extension must be a notification service extension"
    end
    %w[CFBundleShortVersionString CFBundleVersion].each do |key|
      raise ArgumentError, "Extension #{key} must match the app" unless info[key] == app_info[key]
    end
  end

  def self.verify_extension_entitlements!(entitlements)
    raise ArgumentError, "Extension must not include an APNs entitlement" if entitlements.key?("aps-environment")
    verify_keychain_group!(entitlements)
  end
end
