module StoreVersion
  # App Store versions in these states still accept new builds; any other state means Apple approved it.
  OPEN_STATES = %w[
    PREPARE_FOR_SUBMISSION DEVELOPER_REJECTED REJECTED METADATA_REJECTED INVALID_BINARY
    WAITING_FOR_REVIEW IN_REVIEW READY_FOR_REVIEW WAITING_FOR_EXPORT_COMPLIANCE
  ].freeze

  def self.for(version)
    core = version.split(/[-+]/, 2).first
    parts = core.split(".")
    raise ArgumentError, "Unsupported version #{version}: expected MAJOR.MINOR.PATCH" unless parts.length == 3

    major, minor, patch = parts.map { |part| Integer(part, 10) }
    "#{major + 1}.#{minor}.#{patch}"
  end

  def self.beta(version, store_versions)
    approved = store_versions.reject { |_, state| OPEN_STATES.include?(state) }
                             .map { |string, _| Gem::Version.new(string) }.max
    return version if approved.nil? || Gem::Version.new(version) > approved

    major, minor, patch = (approved.segments + [0, 0]).first(3)
    "#{major}.#{minor}.#{patch + 1}"
  end
end
