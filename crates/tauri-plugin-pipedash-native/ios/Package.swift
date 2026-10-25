// swift-tools-version:5.3

import PackageDescription

let package = Package(
  name: "tauri-plugin-pipedash-native",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-pipedash-native",
      type: .static,
      targets: ["tauri-plugin-pipedash-native"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-pipedash-native",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
