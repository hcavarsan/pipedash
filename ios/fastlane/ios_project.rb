require "yaml"

module IosProject
  APP_ID = "com.henrique.pipedash"
  APP_TARGET = "pipedash-desktop_iOS"
  NOTIFICATION_TASK_ID = "#{APP_ID}.notifications-refresh"

  def self.configure(spec, version:, build_number:)
    raise ArgumentError, "Expected a three-part iOS version" unless version.match?(/\A\d+\.\d+\.\d+\z/)
    raise ArgumentError, "Expected a positive build number" unless build_number.to_s.match?(/\A[1-9]\d*\z/)

    app = spec.fetch("targets").fetch(APP_TARGET)
    settings = (app["settings"] ||= {})["base"] ||= {}
    settings.merge!("MARKETING_VERSION" => version, "CURRENT_PROJECT_VERSION" => build_number.to_s)
    properties = app.fetch("info")["properties"] ||= {}
    properties.merge!("CFBundleShortVersionString" => version, "CFBundleVersion" => build_number.to_s)
    spec
  end

  def self.write(path, version:, build_number:)
    spec = YAML.safe_load_file(path, aliases: true)
    configure(spec, version: version, build_number: build_number)
    File.write(path, YAML.dump(spec))
  end

  def self.export_options(profile, team_id:)
    raise ArgumentError, "Missing App Store profile for #{APP_ID}" if profile.to_s.empty?

    {
      method: "app-store-connect",
      signingStyle: "manual",
      teamID: team_id,
      provisioningProfiles: { APP_ID => profile },
    }
  end

  def self.verify_metadata!(app)
    unless app["PipedashBuildWatchEnabled"] == false
      raise ArgumentError, "Build watching and remote push must remain disabled for release"
    end
    if app["NSSupportsLiveActivities"] || app["NSSupportsLiveActivitiesFrequentUpdates"]
      raise ArgumentError, "Live Activities must remain disabled for release"
    end
    unless Array(app["BGTaskSchedulerPermittedIdentifiers"]) == [NOTIFICATION_TASK_ID]
      raise ArgumentError, "App must declare only the notifications background refresh task"
    end
    modes = Array(app["UIBackgroundModes"])
    raise ArgumentError, "App is missing background fetch mode" unless modes.include?("fetch")
    unless (modes & %w[processing remote-notification]).empty?
      raise ArgumentError, "Disabled build watching and push must not declare background modes"
    end
  end

  def self.verify_entitlements!(entitlements)
    raise ArgumentError, "Remote push must not include an APNs entitlement" if entitlements.key?("aps-environment")
  end
end
