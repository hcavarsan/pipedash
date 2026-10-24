// swift-tools-version:5.3

import PackageDescription

let package = Package(
  name: "tauri-plugin-pipedash-push",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-pipedash-push",
      type: .static,
      targets: ["tauri-plugin-pipedash-push"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-pipedash-push",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
