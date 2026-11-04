require "minitest/autorun"
require_relative "push_signing"

class PushSigningTest < Minitest::Test
  Capability = Struct.new(:type) do
    def is_type?(other)
      type == other
    end
  end

  class FakeApp
    attr_reader :created

    def initialize(capabilities)
      @capabilities = capabilities.map { |type| Capability.new(type) }
      @created = []
    end

    def get_capabilities
      @capabilities
    end

    def create_capability(type)
      @created << type
    end
  end

  class FakeBundleIds
    attr_reader :created

    def initialize(apps)
      @apps = apps
      @created = []
    end

    def find(identifier)
      @apps[identifier]
    end

    def create(name:, platform:, identifier:)
      @created << [name, platform, identifier]
    end
  end

  APP = "com.example.app".freeze
  EXTENSION = "com.example.app.NotificationService".freeze

  def test_a_fresh_team_gets_push_and_the_extension_id
    app = FakeApp.new(["APP_ATTEST"])
    bundle_ids = FakeBundleIds.new(APP => app)

    changes = PushSigning.ensure!(bundle_ids: bundle_ids, app_id: APP, extension_id: EXTENSION)

    assert_equal ["PUSH_NOTIFICATIONS"], app.created
    assert_equal [["Pipedash Notification Service", "IOS", EXTENSION]], bundle_ids.created
    assert_equal 2, changes.length
  end

  def test_a_second_run_changes_nothing
    app = FakeApp.new(["PUSH_NOTIFICATIONS"])
    bundle_ids = FakeBundleIds.new(APP => app, EXTENSION => FakeApp.new([]))

    changes = PushSigning.ensure!(bundle_ids: bundle_ids, app_id: APP, extension_id: EXTENSION)

    assert_empty app.created
    assert_empty bundle_ids.created
    assert_empty changes
  end

  def test_a_missing_app_id_stops_before_any_change
    bundle_ids = FakeBundleIds.new({})

    assert_raises(ArgumentError) do
      PushSigning.ensure!(bundle_ids: bundle_ids, app_id: APP, extension_id: EXTENSION)
    end
    assert_empty bundle_ids.created
  end
end
