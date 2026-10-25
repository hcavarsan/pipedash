require "fileutils"
require "json"

module AppIconAppearances
  APPEARANCES = { "light" => nil, "dark" => "dark", "tinted" => "tinted" }.freeze

  def self.variant_path(source_file, appearance)
    suffix = APPEARANCES.fetch(appearance)
    return source_file if suffix.nil?

    ext = File.extname(source_file)
    "#{source_file.delete_suffix(ext)}-#{suffix}#{ext}"
  end

  def self.contents(source_file)
    images = APPEARANCES.keys.map do |appearance|
      image = {
        "filename" => File.basename(variant_path(source_file, appearance)),
        "idiom" => "universal",
        "platform" => "ios",
        "size" => "1024x1024",
      }
      next image if appearance == "light"

      image.merge("appearances" => [{ "appearance" => "luminosity", "value" => appearance }])
    end
    { "images" => images, "info" => { "author" => "xcode", "version" => 1 } }
  end

  def self.write(appiconset, source_file:)
    FileUtils.rm_f(Dir[File.join(appiconset, "*.png")])
    APPEARANCES.each_key { |appearance| FileUtils.cp(variant_path(source_file, appearance), appiconset) }
    File.write(File.join(appiconset, "Contents.json"), JSON.pretty_generate(contents(source_file)) + "\n")
  end
end
