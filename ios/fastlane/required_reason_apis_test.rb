require "minitest/autorun"
require_relative "required_reason_apis"

class RequiredReasonApisTest < Minitest::Test
  def test_detects_symbols_with_and_without_variant_suffix
    found = RequiredReasonApis.detect(
      undefined_symbols: ["_stat$INODE64", "_mach_absolute_time", "_malloc"],
      selectors: [],
    )
    assert_equal ["_stat"], found["NSPrivacyAccessedAPICategoryFileTimestamp"]
    assert_equal ["_mach_absolute_time"], found["NSPrivacyAccessedAPICategorySystemBootTime"]
    refute found.key?("NSPrivacyAccessedAPICategoryDiskSpace")
  end

  def test_does_not_match_symbol_prefixes
    found = RequiredReasonApis.detect(undefined_symbols: ["_statx", "_fstatfs64_x"], selectors: [])
    assert_empty found
  end

  def test_detects_objc_selectors
    found = RequiredReasonApis.detect(undefined_symbols: [], selectors: ["activeInputModes", "init"])
    assert_equal ["activeInputModes"], found["NSPrivacyAccessedAPICategoryActiveKeyboards"]
  end

  def test_undeclared_lists_only_missing_categories
    detected = RequiredReasonApis.detect(
      undefined_symbols: ["_stat", "_statfs"],
      selectors: [],
    )
    manifest = {
      "NSPrivacyAccessedAPITypes" => [
        { "NSPrivacyAccessedAPIType" => "NSPrivacyAccessedAPICategoryFileTimestamp" },
      ],
    }
    assert_equal ["NSPrivacyAccessedAPICategoryDiskSpace"],
                 RequiredReasonApis.undeclared(detected, manifest).keys
  end
end
