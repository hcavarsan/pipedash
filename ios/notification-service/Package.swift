// swift-tools-version:5.9
import PackageDescription

let package = Package(
  name: "notification-service",
  platforms: [.iOS(.v15), .macOS(.v13)],
  products: [.library(name: "AlertFetch", targets: ["AlertFetch"])],
  targets: [
    .target(name: "AlertFetch"),
    .testTarget(name: "AlertFetchTests", dependencies: ["AlertFetch"]),
  ]
)
