require "base64"
require "uri"
require "yaml"

module AppReviewDemo
  class Error < StandardError; end

  def self.verify!(repository:)
    unless repository.match?(/\A[A-Za-z0-9][A-Za-z0-9-]*\/[A-Za-z0-9_.-]+\z/) && !%w[. ..].include?(repository.split("/").last)
      raise Error, "Set APP_REVIEW_DEMO_REPOSITORY to a GitHub owner/repo"
    end

    request = lambda do |path|
      response = yield(path)
      unless response[:status] == 200
        raise Error, "App Review demo check failed (HTTP #{response[:status]}) for #{path}. Check the demo token, repository access, Actions read/write and Contents read permissions."
      end
      response.fetch(:json)
    end

    repo = request.call("/repos/#{repository}")
    unless repo["private"] == false && !repo["archived"] && !repo["disabled"] && repo.dig("permissions", "push")
      raise Error, "App Review needs a public, active demo repository and a token whose user can run and stop its workflows"
    end
    branch = repo.fetch("default_branch")
    ref = URI.encode_www_form(ref: branch)
    workflows = request.call("/repos/#{repository}/actions/workflows?per_page=100").fetch("workflows")
    workflows.select { |workflow| workflow["state"] == "active" }.each do |workflow|
      path = workflow.fetch("path").split("/").map { |part| URI.encode_www_form_component(part) }.join("/")
      source = request.call("/repos/#{repository}/contents/#{path}?#{ref}")
      definition = YAML.safe_load(Base64.strict_decode64(source.fetch("content").delete("\n\r")), aliases: true)
      events = definition["on"] || definition[true]
      dispatchable = case events
                     when Hash then events.key?("workflow_dispatch")
                     when Array then events.include?("workflow_dispatch")
                     else events == "workflow_dispatch"
                     end
      next unless dispatchable
      dispatch = events.is_a?(Hash) ? events["workflow_dispatch"] : nil
      inputs = dispatch.is_a?(Hash) ? dispatch.fetch("inputs", {}) : {}
      next if inputs.values.any? { |input| input["required"] && (input["default"].nil? || input["default"] == "") }

      query = URI.encode_www_form(branch: branch, event: "workflow_dispatch", status: "success", per_page: 1)
      runs = request.call("/repos/#{repository}/actions/workflows/#{workflow.fetch("id")}/runs?#{query}").fetch("workflow_runs")
      next if runs.empty?

      jobs = request.call("/repos/#{repository}/actions/runs/#{runs.first.fetch("id")}/jobs?per_page=1").fetch("jobs")
      next if jobs.empty?

      return workflow.fetch("name")
    end
    raise Error, "Prepare an active workflow_dispatch workflow on the demo repository's default branch with defaults for required inputs, a successful manual run and available jobs, then retry release"
  end
end
