require "set"

module RequiredReasonApis
  CATEGORIES = {
    "NSPrivacyAccessedAPICategoryFileTimestamp" => {
      symbols: %w[_stat _fstat _fstatat _lstat _getattrlist _fgetattrlist _getattrlistat _getattrlistbulk],
      selectors: %w[creationDate modificationDate fileModificationDate contentModificationDateKey
                    creationDateKey],
    },
    "NSPrivacyAccessedAPICategorySystemBootTime" => {
      symbols: %w[_mach_absolute_time],
      selectors: %w[systemUptime],
    },
    "NSPrivacyAccessedAPICategoryDiskSpace" => {
      symbols: %w[_statfs _statvfs _fstatfs _fstatvfs],
      selectors: %w[volumeAvailableCapacityKey volumeAvailableCapacityForImportantUsageKey
                    volumeAvailableCapacityForOpportunisticUsageKey volumeTotalCapacityKey
                    systemFreeSize systemSize],
    },
    "NSPrivacyAccessedAPICategoryActiveKeyboards" => {
      symbols: [],
      selectors: %w[activeInputModes],
    },
    "NSPrivacyAccessedAPICategoryUserDefaults" => {
      symbols: %w[_OBJC_CLASS_$_NSUserDefaults],
      selectors: [],
    },
  }.freeze

  def self.detect(undefined_symbols:, selectors:)
    symbols = undefined_symbols.map { |s| s.sub(/\$[A-Z0-9_]+\z/, "") }.to_set
    selector_set = selectors.to_set
    CATEGORIES.each_with_object({}) do |(category, apis), found|
      hits = apis[:symbols].select { |s| symbols.include?(s) } +
             apis[:selectors].select { |s| selector_set.include?(s) }
      found[category] = hits unless hits.empty?
    end
  end

  def self.declared(manifest)
    Array(manifest["NSPrivacyAccessedAPITypes"]).map { |entry| entry["NSPrivacyAccessedAPIType"] }.to_set
  end

  def self.undeclared(detected, manifest)
    detected.reject { |category, _| declared(manifest).include?(category) }
  end
end
