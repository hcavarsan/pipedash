module StoreVersion
  def self.for(version)
    core = version.split(/[-+]/, 2).first
    parts = core.split(".")
    raise ArgumentError, "Unsupported version #{version}: expected MAJOR.MINOR.PATCH" unless parts.length == 3

    major, minor, patch = parts.map { |part| Integer(part, 10) }
    "#{major + 1}.#{minor}.#{patch}"
  end
end
