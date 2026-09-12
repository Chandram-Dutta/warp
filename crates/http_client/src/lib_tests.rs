use super::*;

#[test]
fn request_preserves_destination_without_cloud_trace_headers() {
    let client = Client::new();
    let request = client
        .get("https://example.com/path?query=value")
        .build()
        .unwrap();
    assert_eq!(
        request.wrapped.url().as_str(),
        "https://example.com/path?query=value"
    );
    assert!(!request.wrapped.headers().contains_key("X-Warp-Traceparent"));
    assert!(!request.wrapped.headers().contains_key("traceparent"));
}
