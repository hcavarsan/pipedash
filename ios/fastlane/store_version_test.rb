require "minitest/autorun"
require_relative "store_version"

class StoreVersionTest < Minitest::Test
  def test_major_is_bumped
    assert_equal "1.5.0", StoreVersion.for("0.5.0")
  end

  def test_prerelease_suffix_is_dropped
    assert_equal "1.5.0", StoreVersion.for("0.5.0-beta.1")
  end

  def test_two_part_version_is_rejected
    assert_raises(ArgumentError) { StoreVersion.for("1.2") }
  end
end
