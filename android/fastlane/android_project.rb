require "fileutils"
require "json"
require "rexml/document"

module AndroidProject
  APP_ID = "com.henrique.pipedash"
  MIN_SDK = 24
  MIN_TARGET_SDK = 36
  MAX_VERSION_CODE = 2_100_000_000
  LAST_SEQUENTIAL_VERSION_CODE = 1005
  RETIRED_VERSION_CODES = [2_026_010_519, 2_026_010_520].freeze
  REFRESH_SERVICE = "com.henrique.pipedash.plugin.NotificationRefreshJob"
  MAIN_ACTIVITY = "com.henrique.pipedash.MainActivity"
  REQUIRED_PERMISSIONS = %w[
    android.permission.INTERNET
    android.permission.POST_NOTIFICATIONS
    android.permission.USE_BIOMETRIC
    android.permission.RECEIVE_BOOT_COMPLETED
    android.permission.ACCESS_NETWORK_STATE
  ].freeze

  def self.config(version:, version_code:)
    raise ArgumentError, "Expected a three-part version" unless version.match?(/\A\d+\.\d+\.\d+\z/)
    unless (1..MAX_VERSION_CODE).cover?(version_code)
      raise ArgumentError, "Expected a version code between 1 and #{MAX_VERSION_CODE}"
    end

    JSON.generate({ version: version, bundle: { android: { versionCode: version_code } } })
  end

  def self.next_version_code(codes)
    ((codes - RETIRED_VERSION_CODES) + [LAST_SEQUENTIAL_VERSION_CODE]).max + 1
  end

  def self.verify_manifest!(xml, version_name:, version_code:)
    manifest = REXML::Document.new(xml).root
    raise ArgumentError, "The manifest is empty" unless manifest

    check = ->(ok, message) { raise ArgumentError, message unless ok }
    attribute = ->(element, name) { element&.attributes&.[]("android:#{name}") }

    check.(manifest.attributes["package"] == APP_ID, "package must be #{APP_ID}")
    check.(attribute.(manifest, "versionName") == version_name, "versionName must be #{version_name}")
    check.(attribute.(manifest, "versionCode") == version_code.to_s, "versionCode must be #{version_code}")

    sdk = manifest.elements["uses-sdk"]
    check.(attribute.(sdk, "minSdkVersion") == MIN_SDK.to_s, "minSdkVersion must be #{MIN_SDK}")
    check.(attribute.(sdk, "targetSdkVersion").to_i >= MIN_TARGET_SDK, "targetSdkVersion must be at least #{MIN_TARGET_SDK}")

    granted = manifest.get_elements("uses-permission").map { |permission| attribute.(permission, "name") }
    missing = REQUIRED_PERMISSIONS - granted
    check.(missing.empty?, "Missing permissions: #{missing.join(", ")}")

    application = manifest.elements["application"]
    check.(application, "The manifest has no application")
    check.(attribute.(application, "allowBackup") == "false", "allowBackup must be false")
    check.(attribute.(application, "dataExtractionRules"), "dataExtractionRules is missing")
    check.(attribute.(application, "debuggable") != "true", "The release must not be debuggable")

    activity = application.get_elements("activity").find { |element| attribute.(element, "name") == MAIN_ACTIVITY }
    check.(activity, "#{MAIN_ACTIVITY} is missing")
    deep_link = activity.get_elements("intent-filter").any? do |filter|
      names = ->(tag) { filter.get_elements(tag).map { |element| attribute.(element, "name") } }
      names.("action").include?("android.intent.action.VIEW") &&
        (%w[android.intent.category.DEFAULT android.intent.category.BROWSABLE] - names.("category")).empty? &&
        filter.get_elements("data").any? { |data| attribute.(data, "scheme") == "pipedash" }
    end
    check.(deep_link, "#{MAIN_ACTIVITY} has no pipedash:// deep link filter")

    service = application.get_elements("service").find { |element| attribute.(element, "name") == REFRESH_SERVICE }
    check.(service, "#{REFRESH_SERVICE} is missing")
    check.(attribute.(service, "process") == ":refresh", "#{REFRESH_SERVICE} must run in :refresh")
    check.(attribute.(service, "permission") == "android.permission.BIND_JOB_SERVICE",
           "#{REFRESH_SERVICE} must require BIND_JOB_SERVICE")
    check.(attribute.(service, "exported") == "false", "#{REFRESH_SERVICE} must not be exported")
  end

  def self.page_aligned?(readelf_output)
    loads = readelf_output.lines.select { |line| line.strip.start_with?("LOAD") }
    !loads.empty? && loads.all? { |line| line.split.last.to_i(16) >= 0x4000 }
  end

  def self.assemble_metadata(source:, images:, into:, changelog: nil, version_code: nil)
    FileUtils.cp_r(File.join(source, "."), into)
    FileUtils.mkdir_p(File.join(into, "en-US/images"))
    FileUtils.cp_r(File.join(images, "."), File.join(into, "en-US/images"))
    return unless changelog

    FileUtils.mkdir_p(File.join(into, "en-US/changelogs"))
    File.write(File.join(into, "en-US/changelogs/#{version_code}.txt"), changelog)
  end
end
