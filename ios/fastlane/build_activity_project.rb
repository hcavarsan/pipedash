require "fileutils"
require "json"
require "open3"
require "pathname"
require "yaml"

module BuildActivityProject
  APP_ID = "com.henrique.pipedash"
  APP_TARGET = "pipedash-desktop_iOS"
  TARGET = "PipedashBuildActivity"
  BUNDLE_ID = "#{APP_ID}.BuildActivity"
  TASK_ID = "#{APP_ID}.build-watch"
  PACKAGE = "tauri-plugin-pipedash-native"
  PRODUCT = "PipedashActivityModels"
  SIGNING_TARGETS = { APP_TARGET => APP_ID, TARGET => BUNDLE_ID }.freeze

  def self.configure(spec, root:, project_dir:, version:, build_number:)
    raise ArgumentError, "Expected a three-part iOS version" unless version.match?(/\A\d+\.\d+\.\d+\z/)
    raise ArgumentError, "Expected a positive build number" unless build_number.to_s.match?(/\A[1-9]\d*\z/)

    targets = spec.fetch("targets")
    app = targets.fetch(APP_TARGET)
    package_path = File.join(root, "crates", PACKAGE, "ios")
    packages = spec["packages"] ||= {}
    package = packages.find do |_, value|
      value["path"] && File.expand_path(value["path"], project_dir) == package_path
    end&.first
    unless package
      package = PACKAGE
      raise ArgumentError, "#{PACKAGE} points to a different Swift package" if packages.key?(package)

      packages[package] = { "path" => Pathname.new(package_path).relative_path_from(Pathname.new(project_dir)).to_s }
    end

    extension = targets[TARGET] ||= {}
    extension.merge!(
      "type" => "app-extension",
      "platform" => "iOS",
      "deploymentTarget" => "16.2",
      "sources" => [{ "path" => Pathname.new(File.join(root, "ios/BuildActivity"))
                                          .relative_path_from(Pathname.new(project_dir)).to_s }],
      "info" => {
        "path" => "#{TARGET}/Info.plist",
        "properties" => {
          "CFBundleDisplayName" => "Pipedash Build",
          "ITSAppUsesNonExemptEncryption" => false,
          "NSExtension" => { "NSExtensionPointIdentifier" => "com.apple.widgetkit-extension" },
        },
      },
      "dependencies" => [{ "package" => package, "product" => PRODUCT }],
    )
    extension["settings"] ||= {}
    extension["settings"]["base"] ||= {}
    extension["settings"]["base"].merge!(
      "PRODUCT_NAME" => TARGET,
      "PRODUCT_BUNDLE_IDENTIFIER" => BUNDLE_ID,
      "APPLICATION_EXTENSION_API_ONLY" => true,
      "SKIP_INSTALL" => true,
      "SWIFT_VERSION" => "5.0",
      "TARGETED_DEVICE_FAMILY" => "1,2",
    )
    dependencies = app["dependencies"] ||= []
    dependencies.reject! { |dependency| dependency["target"] == TARGET }
    dependencies << { "target" => TARGET, "embed" => true, "codeSign" => true }

    [app, extension].each do |target|
      settings = (target["settings"] ||= {})["base"] ||= {}
      settings.merge!("MARKETING_VERSION" => version, "CURRENT_PROJECT_VERSION" => build_number.to_s)
      properties = target.fetch("info")["properties"] ||= {}
      properties.merge!("CFBundleShortVersionString" => version, "CFBundleVersion" => build_number.to_s)
    end
    spec
  end

  def self.write(path, root:, version:, build_number:)
    spec = YAML.safe_load_file(path, aliases: true)
    configure(spec, root: root, project_dir: File.dirname(path), version: version, build_number: build_number)
    File.write(path, YAML.dump(spec))
  end

  def self.cargo_metadata(root:)
    stdout, stderr, status = Open3.capture3("cargo", "metadata", "--locked", "--format-version", "1",
                                          "--filter-platform", "aarch64-apple-ios", chdir: root)
    raise "Cargo metadata failed: #{stderr.strip}" unless status.success?

    JSON.parse(stdout)
  end

  def self.stage_tauri_api(metadata, root:)
    packages = metadata.fetch("packages").select { |package| package.fetch("name") == "tauri" }
    raise ArgumentError, "Expected one resolved Tauri package" unless packages.length == 1

    source = File.join(File.dirname(packages.first.fetch("manifest_path")), "mobile/ios-api")
    raise ArgumentError, "Tauri iOS API package is missing at #{source}" unless File.file?(File.join(source, "Package.swift"))

    destination = File.join(root, "crates", PACKAGE, ".tauri/tauri-api")
    FileUtils.mkdir_p(destination)
    FileUtils.cp_r(File.join(source, "."), destination)
  end

  def self.export_options(profiles, team_id:)
    missing = SIGNING_TARGETS.values.reject { |bundle_id| !profiles[bundle_id].to_s.empty? }
    raise ArgumentError, "Missing App Store profiles for #{missing.join(', ')}" unless missing.empty?

    {
      method: "app-store-connect",
      signingStyle: "manual",
      teamID: team_id,
      provisioningProfiles: SIGNING_TARGETS.values.to_h { |bundle_id| [bundle_id, profiles.fetch(bundle_id)] },
    }
  end

  def self.verify_metadata!(app, extension)
    raise ArgumentError, "App does not support Live Activities" unless app["NSSupportsLiveActivities"] == true
    unless Array(app["BGTaskSchedulerPermittedIdentifiers"]).include?(TASK_ID)
      raise ArgumentError, "App is missing the build-watch background task identifier"
    end
    raise ArgumentError, "App is missing background processing mode" unless Array(app["UIBackgroundModes"]).include?("processing")
    raise ArgumentError, "Build activity extension has the wrong bundle identifier" unless extension["CFBundleIdentifier"] == BUNDLE_ID
    unless extension.dig("NSExtension", "NSExtensionPointIdentifier") == "com.apple.widgetkit-extension"
      raise ArgumentError, "Build activity extension is not a WidgetKit extension"
    end
    %w[CFBundleShortVersionString CFBundleVersion].each do |key|
      raise ArgumentError, "Build activity extension #{key} does not match the app" unless app.fetch(key) == extension.fetch(key)
    end
  end
end
