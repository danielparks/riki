//! Test HTTP server.
#![cfg(test)]

use super::Router;
use crate::rules;
use assert2::assert;
use axum::body::Body;
use axum::extract::Request;
use http::header;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use temp_dir::TempDir;
use tower::ServiceExt;

/// Initialize the test app.
fn init_app() -> (TempDir, PathBuf, axum::Router) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_owned();
    let root_str = root.to_str().expect("TempDir path not UTF-8").to_owned();
    let templates_str = format!("{root_str}/templates");
    let templates: &Path = templates_str.as_ref();

    fs::create_dir(templates).unwrap();
    fs::write(templates.join("default.hbs"), "{{{ body }}}").unwrap();
    fs::write(templates.join("error403.hbs"), "403").unwrap();
    fs::write(templates.join("error404.hbs"), "404").unwrap();
    fs::write(templates.join("error500.hbs"), "{{{ error_debug }}}").unwrap();
    fs::write(
        templates.join("redirect301.hbs"),
        "redirect {{ canonical_url }}",
    )
    .unwrap();

    let router = Arc::new(Router::from_configuration(
        rules::default_rules(root_str, templates_str).unwrap(),
    ));

    let app = axum::Router::new()
        .route("/{*path}", axum::routing::get(super::path_handler))
        .route("/", axum::routing::get(super::path_handler))
        .with_state(router);

    (temp_dir, root, app)
}

/// A summarized response that can be compared for easy assertions.
#[derive(Debug, Eq, PartialEq)]
struct Response {
    status: http::StatusCode,
    content_type: Option<mime::Mime>,
    last_modified: bool,
    etag: bool,
    location: Option<String>,
    body: Vec<u8>,
}

impl Response {
    /// Create the summary from an Axum response.
    async fn from(resp: axum::response::Response) -> Self {
        let status = resp.status();
        let headers = resp.headers();

        let content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok());
        let last_modified = headers.contains_key(header::LAST_MODIFIED);
        let etag = headers.contains_key(header::ETAG);
        let location = headers
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();

        Self { status, content_type, last_modified, etag, location, body }
    }

    /// An expected 301 Moved Permanently response.
    fn redirect(to: &str) -> Self {
        Self {
            status: http::StatusCode::MOVED_PERMANENTLY,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: Some(to.to_owned()),
            body: format!("redirect {to}").into_bytes(),
        }
    }

    /// An expected response for an HTML page.
    fn page_html(body: &str) -> Self {
        Self {
            status: http::StatusCode::OK,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: body.as_bytes().to_vec(),
        }
    }

    /// An expected response for a Markdown file (a page source).
    fn page_source(body: &str) -> Self {
        Self {
            status: http::StatusCode::OK,
            content_type: Some("text/markdown; charset=utf-8".parse().unwrap()),
            last_modified: false,
            etag: false,
            location: None,
            body: body.as_bytes().to_vec(),
        }
    }

    /// An expected response for a static HTML file.
    fn static_html(body: &str) -> Self {
        Self::static_other(body, mime::TEXT_HTML_UTF_8)
    }

    /// An expected response for a static file of type `content_type`.
    fn static_other(body: &str, content_type: mime::Mime) -> Self {
        Self {
            status: http::StatusCode::OK,
            content_type: Some(content_type),
            last_modified: true,
            etag: true,
            location: None,
            body: body.as_bytes().to_vec(),
        }
    }
}

/// Make a GET request to the test app.
async fn get(app: &axum::Router, uri: &str) -> Response {
    Response::from(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await
}

#[tokio::test]
#[test_log::test]
async fn test_directory_page_get() {
    let (_dir, root, app) = init_app();

    fs::write(root.join("index.md"), "index").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/index.md"), "DIR").unwrap();

    assert!(
        Response::page_html(
            "<html><head></head><body><p>index</p>\n</body></html>"
        ) == get(&app, "/").await
    );
    assert!(Response::redirect("/") == get(&app, "/.").await);
    assert!(Response::redirect("/") == get(&app, "/index").await);
    assert!(Response::page_source("index") == get(&app, "/index.md").await);
    assert!(Response::redirect("/index.md") == get(&app, "/index.md/").await);

    assert!(
        Response::page_html(
            "<html><head></head><body><p>DIR</p>\n</body></html>"
        ) == get(&app, "/dir/").await
    );
    assert!(Response::redirect("/dir/") == get(&app, "/dir").await);
    assert!(Response::redirect("/dir/") == get(&app, "/dir/.").await);
    assert!(Response::redirect("/dir/") == get(&app, "/dir/index").await);
    assert!(Response::redirect("/dir/") == get(&app, "/dir/././index").await);
    assert!(Response::page_source("DIR") == get(&app, "/dir/index.md").await);
}

#[tokio::test]
#[test_log::test]
async fn test_file_page_get() {
    let (_dir, root, app) = init_app();

    fs::write(root.join("page.md"), "PAGE").unwrap();

    assert!(
        Response::page_html(
            "<html><head></head><body><p>PAGE</p>\n</body></html>"
        ) == get(&app, "/page").await
    );
    assert!(Response::redirect("/page") == get(&app, "/page/").await);
    assert!(Response::redirect("/page") == get(&app, "/page/.").await);
    assert!(Response::redirect("/page") == get(&app, "/page/././").await);
    assert!(Response::page_source("PAGE") == get(&app, "/page.md").await);
}

#[tokio::test]
#[test_log::test]
async fn test_static_file_get() {
    let (_dir, root, app) = init_app();

    fs::write(root.join("a.txt"), "AAA").unwrap();

    assert!(
        Response::static_other("AAA", mime::TEXT_PLAIN_UTF_8)
            == get(&app, "/a.txt").await
    );
    assert!(Response::redirect("/a.txt") == get(&app, "/a.txt/").await);
    assert!(Response::redirect("/a.txt") == get(&app, "/a.txt/.").await);
    assert!(Response::redirect("/a.txt") == get(&app, "/a.txt/././").await);
}

#[tokio::test]
#[test_log::test]
async fn test_static_directory_get() {
    let (_dir, root, app) = init_app();

    fs::create_dir(root.join("b")).unwrap();
    fs::write(root.join("b/index.html"), "BBB").unwrap();

    assert!(Response::static_html("BBB") == get(&app, "/b/").await);
    assert!(Response::redirect("/b/") == get(&app, "/b").await);
    assert!(Response::redirect("/b/") == get(&app, "/b/index.html").await);
    assert!(Response::redirect("/b/") == get(&app, "/b/././index.html").await);
}

#[tokio::test]
#[test_log::test]
async fn test_static_index_with_page() {
    let (_dir, root, app) = init_app();

    fs::create_dir(root.join("static")).unwrap();
    fs::write(root.join("static/index.html"), "STATIC").unwrap();
    fs::write(root.join("static/page.md"), "PAGE").unwrap();

    assert!(Response::redirect("/static/") == get(&app, "/static").await);
    assert!(Response::static_html("STATIC") == get(&app, "/static/").await);
    assert!(
        Response::page_html(
            "<html><head></head><body><p>PAGE</p>\n</body></html>"
        ) == get(&app, "/static/page").await
    );
    assert!(
        Response::redirect("/static/page") == get(&app, "/static/page/").await
    );
    assert!(
        Response::page_source("PAGE") == get(&app, "/static/page.md").await
    );
}

#[tokio::test]
#[test_log::test]
async fn test_static_hides_page() {
    let (_dir, root, app) = init_app();

    fs::write(root.join("index.html"), "STATIC").unwrap();
    fs::write(root.join("index.md"), "PAGE").unwrap();

    assert!(Response::static_html("STATIC") == get(&app, "/").await);
    assert!(Response::redirect("/") == get(&app, "/index.html").await);
    assert!(Response::page_source("PAGE") == get(&app, "/index.md").await);
}

#[tokio::test]
#[test_log::test]
async fn test_not_found_get() {
    let (_dir, _root, app) = init_app();

    assert!(
        Response {
            status: http::StatusCode::NOT_FOUND,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: b"404".to_vec(),
        } == get(&app, "/not-found").await
    );
}

#[cfg(all(not(target_os = "hermit"), unix))]
#[tokio::test]
#[test_log::test]
async fn test_forbidden_page_get() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, root, app) = init_app();

    let path = root.join("forbidden.md");
    fs::write(&path, "forbidden").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

    assert!(
        Response {
            status: http::StatusCode::FORBIDDEN,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: b"403".to_vec(),
        } == get(&app, "/forbidden").await
    );

    assert!(
        Response {
            status: http::StatusCode::FORBIDDEN,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: b"403".to_vec(),
        } == get(&app, "/forbidden.md").await
    );
}

#[cfg(all(not(target_os = "hermit"), unix))]
#[tokio::test]
#[test_log::test]
async fn test_forbidden_static_get() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, root, app) = init_app();

    let path = root.join("forbidden.txt");
    fs::write(&path, "forbidden").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

    assert!(
        Response {
            status: http::StatusCode::FORBIDDEN,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: b"403".to_vec(),
        } == get(&app, "/forbidden.txt").await
    );
}
