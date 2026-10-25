require "minitest/autorun"
require "tmpdir"
require_relative "app_icon_appearances"

class AppIconAppearancesTest < Minitest::Test
  def test_the_catalog_has_one_any_dark_and_tinted_icon
    images = AppIconAppearances.contents("icons/ios/app-icon-1024.png")["images"]

    assert_equal(%w[app-icon-1024.png app-icon-1024-dark.png app-icon-1024-tinted.png],
                 images.map { |image| image["filename"] })
    assert_nil images[0]["appearances"]
    assert_equal([{ "appearance" => "luminosity", "value" => "dark" }], images[1]["appearances"])
    assert_equal([{ "appearance" => "luminosity", "value" => "tinted" }], images[2]["appearances"])
    assert(images.all? { |image| image["size"] == "1024x1024" && image["platform"] == "ios" })
  end

  def test_write_replaces_the_generated_icons_with_the_three_appearances
    Dir.mktmpdir do |dir|
      source = File.join(dir, "app-icon-1024.png")
      %w[app-icon-1024.png app-icon-1024-dark.png app-icon-1024-tinted.png].each do |name|
        File.write(File.join(dir, name), name)
      end
      appiconset = File.join(dir, "AppIcon.appiconset")
      FileUtils.mkdir_p(appiconset)
      File.write(File.join(appiconset, "AppIcon-20x20@1x.png"), "old")

      AppIconAppearances.write(appiconset, source_file: source)

      assert_equal(%w[Contents.json app-icon-1024-dark.png app-icon-1024-tinted.png app-icon-1024.png],
                   Dir.children(appiconset).sort)
      assert_equal 3, JSON.parse(File.read(File.join(appiconset, "Contents.json")))["images"].length
    end
  end
end
