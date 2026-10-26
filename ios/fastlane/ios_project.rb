require "yaml"

module IosProject
  APP_ID = "com.henrique.pipedash"
  APP_TARGET = "pipedash-desktop_iOS"
  TASK_ID = "#{APP_ID}.build-watch"

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
    unless Array(app["BGTaskSchedulerPermittedIdentifiers"]).include?(TASK_ID)
      raise ArgumentError, "App is missing the build-watch background task identifier"
    end
    raise ArgumentError, "App is missing background processing mode" unless Array(app["UIBackgroundModes"]).include?("processing")
  end
end
