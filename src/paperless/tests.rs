use super::{ base_domain, normalize_next_url, parse_duplicate_document_id };

#[test]
fn duplicate_id_is_parsed_from_paperless_error_message() {
    assert_eq!(
        parse_duplicate_document_id(
            "moin moin.md: Not consuming moin moin.md: It is a duplicate of moin moin (#327)."
        ),
        Some(327)
    );
}

#[test]
fn other_error_messages_yield_no_id() {
    assert_eq!(parse_duplicate_document_id("Connection timed out"), None);
}

#[test]
fn next_url_takes_scheme_from_configured_base() {
    // The real-world case: Paperless behind a proxy without X-Forwarded-Proto
    // returns http:// even though the instance is reachable over https://.
    assert_eq!(
        normalize_next_url(
            "http://paperless.example.dev/api/documents/?page=2&page_size=100",
            "https://paperless.example.dev"
        ),
        "https://paperless.example.dev/api/documents/?page=2&page_size=100"
    );
}

#[test]
fn lan_instance_stays_http_with_port() {
    assert_eq!(
        normalize_next_url(
            "http://paperless.local:8000/api/documents/?page=2",
            "http://paperless.local:8000"
        ),
        "http://paperless.local:8000/api/documents/?page=2"
    );
}

#[test]
fn missing_slash_before_query_is_added() {
    assert_eq!(
        normalize_next_url("https://p.example.dev/api/documents?page=2", "https://p.example.dev"),
        "https://p.example.dev/api/documents/?page=2"
    );
}

#[test]
fn base_domain_strips_api_path_and_keeps_port() {
    assert_eq!(base_domain("http://paperless.local:8000/api/documents/"), "http://paperless.local:8000");
    assert_eq!(base_domain("https://paperless.example.dev/"), "https://paperless.example.dev");
}
