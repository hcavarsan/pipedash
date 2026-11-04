import XCTest

@testable import AlertFetch

final class AlertFetchTests: XCTestCase {
  private func allowed(_ url: String, _ allowPrivateHttp: Bool) -> Bool {
    AlertFetch.isAllowed(URL(string: url)!, allowPrivateHttp: allowPrivateHttp)
  }

  func testPolicyTable() {
    XCTAssertTrue(allowed("https://8.8.8.8", false))
    XCTAssertTrue(allowed("https://host.example.com", false))
    XCTAssertFalse(allowed("http://192.168.1.5:8080", false))
    XCTAssertTrue(allowed("http://192.168.1.5:8080", true))
    XCTAssertTrue(allowed("http://mac.local", true))
    XCTAssertFalse(allowed("http://mac.local", false))
    XCTAssertTrue(allowed("http://[::1]:8080", true))
    XCTAssertTrue(allowed("http://[fd00::1]", true))
    XCTAssertTrue(allowed("http://[fe80::1]", true))
    XCTAssertFalse(allowed("http://[2001:4860:4860::8888]", true))
    XCTAssertFalse(allowed("http://ci.lan", true))
    XCTAssertFalse(allowed("http://nas.internal", true))
    XCTAssertFalse(allowed("http://nas", true))
    XCTAssertFalse(allowed("http://8.8.8.8", true))
    XCTAssertFalse(allowed("ftp://192.168.1.5", true))
  }

  func testPrivateRanges() {
    for host in ["10.0.0.1", "172.16.0.1", "172.31.255.255", "192.168.0.1", "100.64.0.1", "100.127.0.1", "169.254.1.1", "127.0.0.1"] {
      XCTAssertTrue(AlertFetch.isPrivateLiteral(host), host)
    }
    for host in ["172.15.0.1", "172.32.0.1", "100.63.0.1", "100.128.0.1", "11.0.0.1", "192.169.0.1", "8.8.8.8", "example.com"] {
      XCTAssertFalse(AlertFetch.isPrivateLiteral(host), host)
    }
  }

  func testAlertIdValidation() {
    XCTAssertTrue(AlertFetch.isValidAlertId("0123456789abcdef0123456789abcdef"))
    XCTAssertTrue(AlertFetch.isValidAlertId("a-B_9"))
    XCTAssertTrue(AlertFetch.isValidAlertId(String(repeating: "a", count: 64)))
    XCTAssertFalse(AlertFetch.isValidAlertId(""))
    XCTAssertFalse(AlertFetch.isValidAlertId(String(repeating: "a", count: 65)))
    XCTAssertFalse(AlertFetch.isValidAlertId("../etc"))
    XCTAssertFalse(AlertFetch.isValidAlertId("a/b"))
    XCTAssertFalse(AlertFetch.isValidAlertId("a?b"))
  }

  func testRequestShape() throws {
    let config = try XCTUnwrap(
      AlertFetch.config(
        from: Data(
          #"{"server_url":"https://host.example.com/base/","token":"t0k","allow_private_http":false,"device_id":"d"}"#
            .utf8)))
    let request = try XCTUnwrap(AlertFetch.request(config: config, alertId: "abc123"))
    XCTAssertEqual(request.url?.absoluteString, "https://host.example.com/base/api/v1/alerts/abc123")
    XCTAssertEqual(request.httpMethod, "GET")
    XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer t0k")
    XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
    XCTAssertEqual(request.timeoutInterval, AlertFetch.timeout)
    XCTAssertNil(AlertFetch.request(config: config, alertId: "../x"))
  }

  func testRequestHonoursTransportPolicy() throws {
    let json = { (flag: Bool) in
      Data(#"{"server_url":"http://192.168.1.5:8080","token":"t","allow_private_http":\#(flag)}"#.utf8)
    }
    let denied = try XCTUnwrap(AlertFetch.config(from: json(false)))
    let permitted = try XCTUnwrap(AlertFetch.config(from: json(true)))
    XCTAssertNil(AlertFetch.request(config: denied, alertId: "abc"))
    XCTAssertNotNil(AlertFetch.request(config: permitted, alertId: "abc"))
  }

  func testDecode() throws {
    let data = Data(
      #"{"id":"abc","seq":4,"key":"pipeline:p1","kind":"failed","title":"T","body":"B","pipeline_id":"p1","provider_id":7,"created_at":"2026-11-03T00:00:00Z"}"#
        .utf8)
    let alert = try XCTUnwrap(AlertFetch.decode(data))
    XCTAssertEqual(alert.key, "pipeline:p1")
    XCTAssertEqual(alert.kind, "failed")
    XCTAssertEqual(alert.pipelineId, "p1")
    XCTAssertEqual(alert.providerId, 7)

    let bare = try XCTUnwrap(
      AlertFetch.decode(
        Data(
          #"{"id":"a","key":"k","kind":"outage","title":"T","body":"B","pipeline_id":null,"provider_id":null}"#
            .utf8)))
    XCTAssertNil(bare.pipelineId)
    XCTAssertNil(bare.providerId)
    XCTAssertNil(AlertFetch.decode(Data("{}".utf8)))
  }

  func testHostConfigDecodesStandaloneAndDefaultsToFalse() throws {
    let json = { (extra: String) in
      Data(#"{"server_url":"https://h.example.com","token":"t","allow_private_http":false\#(extra)}"#.utf8)
    }
    XCTAssertTrue(try XCTUnwrap(AlertFetch.config(from: json(#","standalone":true"#))).standalone)
    XCTAssertFalse(try XCTUnwrap(AlertFetch.config(from: json(#","standalone":false"#))).standalone)
    XCTAssertFalse(try XCTUnwrap(AlertFetch.config(from: json(""))).standalone)
  }

  func testRouteNamespacesStandaloneAlertsAndDropsServerIds() throws {
    let alert = try XCTUnwrap(
      AlertFetch.decode(
        Data(
          #"{"id":"a","key":"pipeline:p1","kind":"failed","title":"T","body":"B","pipeline_id":"p1","provider_id":7}"#
            .utf8)))
    XCTAssertEqual(
      AlertFetch.route(for: alert, standalone: true),
      AlertRoute(key: "alert-host:pipeline:p1", pipelineId: nil, providerId: nil))
    XCTAssertEqual(
      AlertFetch.route(for: alert, standalone: false),
      AlertRoute(key: "pipeline:p1", pipelineId: "p1", providerId: 7))
  }
}
