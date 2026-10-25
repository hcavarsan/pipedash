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
      targets: ["tauri-plugin-pipedash-native"]),
    .library(name: "PipedashActivityModels", targets: ["PipedashActivityModels"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(name: "PipedashActivityModels", path: "ActivityModels"),
    .target(
      name: "tauri-plugin-pipedash-native",
      dependencies: [
        .byName(name: "Tauri"),
        .byName(name: "PipedashActivityModels")
      ],
      path: "Sources"),
    .testTarget(
      name: "PipedashNativeTests",
      dependencies: ["tauri-plugin-pipedash-native", "PipedashActivityModels"],
      path: "Tests")
  ]
)
