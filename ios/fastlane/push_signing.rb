module PushSigning
  PUSH_NOTIFICATIONS = "PUSH_NOTIFICATIONS".freeze
  EXTENSION_NAME = "Pipedash Notification Service".freeze

  def self.ensure!(bundle_ids:, app_id:, extension_id:)
    app = bundle_ids.find(app_id)
    raise ArgumentError, "App ID #{app_id} is not registered for this team" if app.nil?

    changes = []
    unless app.get_capabilities.any? { |capability| capability.is_type?(PUSH_NOTIFICATIONS) }
      app.create_capability(PUSH_NOTIFICATIONS)
      changes << "Turned on Push Notifications for #{app_id}"
    end
    if bundle_ids.find(extension_id).nil?
      bundle_ids.create(name: EXTENSION_NAME, platform: "IOS", identifier: extension_id)
      changes << "Registered App ID #{extension_id}"
    end
    changes
  end
end
