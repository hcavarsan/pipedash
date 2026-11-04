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

  def test_beta_keeps_the_version_while_its_train_is_open
    assert_equal "1.9.0", StoreVersion.beta("1.9.0", [["1.8.0", "READY_FOR_SALE"]])
    assert_equal "1.8.0", StoreVersion.beta("1.8.0", [["1.7.0", "READY_FOR_SALE"], ["1.8.0", "WAITING_FOR_REVIEW"]])
    assert_equal "1.8.0", StoreVersion.beta("1.8.0", [])
  end

  def test_beta_moves_past_the_highest_approved_version
    assert_equal "1.8.1", StoreVersion.beta("1.8.0", [["1.8.0", "READY_FOR_SALE"], ["1.7.0", "REPLACED_WITH_NEW_VERSION"]])
    assert_equal "1.8.1", StoreVersion.beta("1.8.0", [["1.8.0", "PENDING_DEVELOPER_RELEASE"]])
    assert_equal "1.10.1", StoreVersion.beta("1.9.0", [["1.10.0", "READY_FOR_DISTRIBUTION"], ["1.9.0", "READY_FOR_SALE"]])
    assert_equal "2.0.1", StoreVersion.beta("1.9.0", [["2.0", "READY_FOR_SALE"]])
  end
end
