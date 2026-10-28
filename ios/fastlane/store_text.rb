module StoreText
  class Error < StandardError; end

  WHATS_NEW_LIMIT = 4000
  PROMOTIONAL_LIMIT = 170
  DROPPED_ITEMS = /\A(bump version|update changelog)\b/i

  # The file for the iOS version wins; otherwise the GitHub release of the tag.
  def self.whats_new(version:, tag:, dir:)
    path = File.join(dir, "#{version}.txt")
    text = if File.exist?(path)
             File.read(path).strip
           else
             response = yield("/repos/#{ENV.fetch("GITHUB_REPOSITORY", "hcavarsan/pipedash")}/releases/tags/#{tag}")
             unless response[:status] == 200
               raise Error, "No published GitHub release #{tag} (HTTP #{response[:status]}). Publish it or add #{path}"
             end
             plain(response.fetch(:json).fetch("body").to_s)
           end
    raise Error, "What's New for #{version} is empty. Add text to #{path}" if text.empty?

    text.length > WHATS_NEW_LIMIT ? clip(text) : text
  end

  def self.promotional_text(version:, dir:)
    path = File.join(dir, "#{version}.promo.txt")
    return nil unless File.exist?(path)

    text = File.read(path).strip
    unless (1..PROMOTIONAL_LIMIT).cover?(text.length)
      raise Error, "#{path} must have 1 to #{PROMOTIONAL_LIMIT} characters, it has #{text.length}"
    end
    text
  end

  # GitHub release markdown to App Store plain text: paragraphs, section titles and "- " items.
  def self.plain(markdown)
    blocks = []
    section = nil
    in_list = false
    markdown.gsub(/<!--.*?-->/m, "").each_line do |raw|
      line = raw.rstrip
      next if line.strip.empty? || line.match?(/\A\s*!\[|\A\[[^\]]+\]:\s|\A##?\s/)

      if (title = line[/\A###+\s*(.+)\z/, 1])
        section = inline(title)
        next
      end
      text = inline(line)
      if (item = line[/\A[-*]\s+(.+)\z/, 1]&.then { |raw_item| inline(raw_item) })
        next if item.match?(DROPPED_ITEMS)

        if section || !in_list
          blocks << [section].compact
          section = nil
          in_list = true
        end
        blocks.last << "- #{item}"
      elsif !text.start_with?(" ")
        blocks << [text]
        in_list = false
      end
    end
    blocks.map { |block| block.join("\n") }.join("\n\n")
  end

  def self.inline(text)
    text.gsub(/!\[[^\]]*\]\([^)]*\)/, "")
        .gsub(/\[([^\]]+)\]\([^)]*\)/, '\1')
        .gsub(/\s*\((?:#\d+|[0-9a-f]{7,40})(?:,\s*(?:#\d+|[0-9a-f]{7,40}))*\)/, "")
        .delete("`*")
        .rstrip
  end

  def self.clip(text)
    kept = []
    text.lines.each do |line|
      break if (kept.join + line).rstrip.length > WHATS_NEW_LIMIT - 2

      kept << line
    end
    "#{kept.join.rstrip}\n…"
  end
end
