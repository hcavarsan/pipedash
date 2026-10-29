require "minitest/autorun"
require "tmpdir"
require_relative "android_project"

class AndroidProjectTest < Minitest::Test
  MANIFEST = <<~XML
    <manifest xmlns:android="http://schemas.android.com/apk/res/android" android:versionCode="8" android:versionName="1.5.0" package="com.henrique.pipedash">
      <uses-sdk android:minSdkVersion="24" android:targetSdkVersion="37"/>
      <uses-permission android:name="android.permission.INTERNET"/>
      <uses-permission android:name="android.permission.POST_NOTIFICATIONS"/>
      <uses-permission android:name="android.permission.USE_BIOMETRIC"/>
      <uses-permission android:name="android.permission.RECEIVE_BOOT_COMPLETED"/>
      <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE"/>
      <application android:allowBackup="false" android:dataExtractionRules="@xml/pipedash_data_extraction_rules" android:debuggable="false">
        <activity android:name="com.henrique.pipedash.MainActivity" android:exported="true">
          <intent-filter>
            <action android:name="android.intent.action.MAIN"/>
            <category android:name="android.intent.category.LAUNCHER"/>
          </intent-filter>
          <intent-filter>
            <action android:name="android.intent.action.VIEW"/>
            <category android:name="android.intent.category.DEFAULT"/>
            <category android:name="android.intent.category.BROWSABLE"/>
            <data android:scheme="pipedash"/>
          </intent-filter>
        </activity>
        <service android:name="com.henrique.pipedash.plugin.NotificationRefreshJob" android:exported="false" android:permission="android.permission.BIND_JOB_SERVICE" android:process=":refresh"/>
      </application>
    </manifest>
  XML

  def verify(xml)
    AndroidProject.verify_manifest!(xml, version_name: "1.5.0", version_code: 8)
  end

  def assert_rejected(xml, message)
    error = assert_raises(ArgumentError) { verify(xml) }
    assert_includes error.message, message
  end

  def test_config_accepts_a_version_and_code
    assert_equal({ "version" => "1.5.0", "bundle" => { "android" => { "versionCode" => 8 } } },
                 JSON.parse(AndroidProject.config(version: "1.5.0", version_code: 8)))
  end

  def test_config_rejects_bad_codes_and_versions
    assert_raises(ArgumentError) { AndroidProject.config(version: "1.5.0", version_code: 0) }
    assert_raises(ArgumentError) { AndroidProject.config(version: "1.5.0", version_code: AndroidProject::MAX_VERSION_CODE + 1) }
    assert_raises(ArgumentError) { AndroidProject.config(version: "1.2", version_code: 1) }
  end

  def test_manifest_is_accepted
    verify(MANIFEST)
  end

  def test_wrong_version_name_is_rejected
    assert_rejected(MANIFEST.sub('versionName="1.5.0"', 'versionName="0.5.0"'), "versionName")
  end

  def test_missing_allow_backup_is_rejected
    assert_rejected(MANIFEST.sub('android:allowBackup="false" ', ""), "allowBackup")
  end

  def test_low_target_sdk_is_rejected
    assert_rejected(MANIFEST.sub('targetSdkVersion="37"', 'targetSdkVersion="35"'), "targetSdkVersion")
  end

  def test_missing_deep_link_filter_is_rejected
    assert_rejected(MANIFEST.sub('android:scheme="pipedash"', 'android:scheme="other"'), "deep link")
  end

  def test_refresh_service_outside_its_process_is_rejected
    assert_rejected(MANIFEST.sub(' android:process=":refresh"', ""), ":refresh")
  end

  def test_debuggable_release_is_rejected
    assert_rejected(MANIFEST.sub('debuggable="false"', 'debuggable="true"'), "debuggable")
  end

  def test_page_alignment
    load_line = ->(align) { "  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x0a1b2c 0x0a1b2c R E #{align}\n" }
    assert AndroidProject.page_aligned?("Program Headers:\n#{load_line.("0x4000")}  GNU_STACK 0x0 0x0 0x0 0x0 0x0 RW 0x10\n")
    refute AndroidProject.page_aligned?(load_line.("0x4000") + load_line.("0x1000"))
    refute AndroidProject.page_aligned?("Program Headers:\n  GNU_STACK 0x0 0x0 0x0 0x0 0x0 RW 0x10\n")
  end

  def test_next_version_code_continues_the_sequential_codes
    assert_equal 1006, AndroidProject.next_version_code([])
    assert_equal 1006, AndroidProject.next_version_code([1001, 2_026_010_519, 2_026_010_520])
    assert_equal 1021, AndroidProject.next_version_code([1010, 1020, 2_026_010_520])
  end

  def test_release_builds_are_stripped_with_the_pinned_ndk_and_keep_symbol_tables_once
    gradle = <<~GRADLE
      android {
          compileSdk = 37
          buildTypes {
              getByName("debug") {
                  packaging {
                      jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                      jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
                  }
              }
              getByName("release") {
                  optimization {}
              }
          }
      }
    GRADLE
    patched = AndroidProject.with_debug_symbols(gradle, ndk_version: "30.0.1")

    assert patched.start_with?("android {\n    ndkVersion = \"30.0.1\"\n    compileSdk = 37")
    assert_includes patched, "getByName(\"release\") {\n            ndk { debugSymbolLevel = \"SYMBOL_TABLE\" }\n            optimization {}"
    refute_includes patched, "keepDebugSymbols"
    assert_equal patched, AndroidProject.with_debug_symbols(patched, ndk_version: "30.0.1")
    assert_equal 1, AndroidProject.with_debug_symbols(patched, ndk_version: "31.0.0").scan("ndkVersion").size
    assert_raises(ArgumentError) { AndroidProject.with_debug_symbols("buildTypes {}", ndk_version: "30.0.1") }
  end

  def test_metadata_keeps_the_icon_and_gets_a_changelog
    Dir.mktmpdir do |dir|
      source = File.join(dir, "source")
      images = File.join(dir, "images")
      into = File.join(dir, "into")
      FileUtils.mkdir_p([File.join(source, "en-US/images"), File.join(images, "phoneScreenshots"), into])
      File.write(File.join(source, "en-US/images/icon.png"), "icon")
      File.write(File.join(source, "en-US/title.txt"), "Pipedash")
      File.write(File.join(images, "phoneScreenshots/01.png"), "shot")

      AndroidProject.assemble_metadata(source: source, images: images, into: into, changelog: "New", version_code: 8)

      assert_equal "icon", File.read(File.join(into, "en-US/images/icon.png"))
      assert_equal "shot", File.read(File.join(into, "en-US/images/phoneScreenshots/01.png"))
      assert_equal "New", File.read(File.join(into, "en-US/changelogs/8.txt"))
    end
  end
end
