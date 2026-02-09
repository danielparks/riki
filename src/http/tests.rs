//! Test HTTP server.
#![cfg(test)]

use super::{Configuration, Router, path_handler};
use actix_http::{Request, header};
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::http::header::HeaderName;
use actix_web::test::TestRequest;
use actix_web::web::{Bytes, Data};
use actix_web::{App, body, http, test};
use assert2::assert;
use std::fs;
use temp_dir::TempDir;
use tracing_test::traced_test;

/// Initialize the test app.
#[expect(clippy::future_not_send, reason = "Required by Actix")]
async fn init_app() -> (
    TempDir,
    Configuration,
    impl Service<
        Request,
        Response = ServiceResponse<BoxBody>,
        Error = actix_web::Error,
    >,
) {
    let temp_dir = TempDir::new().unwrap();
    let config = Configuration::default_in(temp_dir.path());
    let tpls_dir = &config.templates_path;
    fs::create_dir(tpls_dir).unwrap();
    fs::write(tpls_dir.join("default.hbs"), "{{{ body }}}").unwrap();
    fs::write(tpls_dir.join("error403.hbs"), "403").unwrap();
    fs::write(tpls_dir.join("error404.hbs"), "404").unwrap();
    fs::write(tpls_dir.join("error500.hbs"), "{{{ error_debug }}}").unwrap();
    fs::write(
        tpls_dir.join("redirect301.hbs"),
        "redirect {{ canonical_url }}",
    )
    .unwrap();

    let router = Data::new(Router::from_configuration(config.clone()));
    (
        temp_dir,
        config,
        test::init_service(App::new().app_data(router).service(path_handler))
            .await,
    )
}

/// Get body of response
#[expect(clippy::future_not_send, reason = "Required by Actix")]
async fn get_body(resp: ServiceResponse<BoxBody>) -> Bytes {
    body::to_bytes(resp.into_body()).await.unwrap()
}

/// Get the value of a response header
fn get_header(
    resp: &ServiceResponse<BoxBody>,
    name: HeaderName,
) -> Option<Bytes> {
    resp.headers()
        .get(name)
        .map(|v| v.as_bytes().to_vec().into())
}

/// Get the content-type of a response
fn get_content_type(resp: &ServiceResponse<BoxBody>) -> Option<mime::Mime> {
    resp.headers().get(header::CONTENT_TYPE).map(|v| {
        v.to_str()
            .expect("content-type to be ASCII")
            .parse()
            .unwrap()
    })
}

/// Convert byte string to right type for comparison with a response body.
const B: fn(&'static [u8]) -> Bytes = Bytes::from_static;

/// A summarized response that can be compared for easy assertions.
#[derive(Debug, Eq, PartialEq)]
struct Response {
    status: http::StatusCode,
    content_type: Option<mime::Mime>,
    last_modified: bool,
    etag: bool,
    location: Option<String>,
    body: Bytes,
}

impl Response {
    /// Create the summary from an actual [`ServiceResponse`].
    #[expect(clippy::future_not_send, reason = "Required by Actix")]
    async fn from(resp: ServiceResponse<BoxBody>) -> Self {
        Self {
            status: resp.status(),
            content_type: get_content_type(&resp),
            last_modified: get_header(&resp, header::LAST_MODIFIED).is_some(),
            etag: get_header(&resp, header::ETAG).is_some(),
            location: get_header(&resp, header::LOCATION)
                .map(|v| v.to_vec().try_into().expect("Location not UTF-8")),
            body: get_body(resp).await,
        }
    }

    /// An expected 301 Moved Permanently response.
    fn redirect(to: &str) -> Self {
        Self {
            status: http::StatusCode::MOVED_PERMANENTLY,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: Some(to.to_owned()),
            body: format!("redirect {to}").into(),
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
            body: body.to_owned().into(),
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
            body: body.to_owned().into(),
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
            body: body.to_owned().into(),
        }
    }
}

/// Make a GET request to the test app.
#[expect(clippy::future_not_send, reason = "Required by Actix")]
async fn get<S, E>(app: S, uri: &str) -> Response
where
    S: Service<Request, Response = ServiceResponse<BoxBody>, Error = E>,
    E: std::fmt::Debug,
{
    Response::from(
        test::call_service(&app, TestRequest::get().uri(uri).to_request())
            .await,
    )
    .await
}

#[actix_web::test]
#[traced_test]
async fn test_directory_page_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.root_path.join("index.md"), "index").unwrap();
    fs::create_dir(config.root_path.join("dir")).unwrap();
    fs::write(config.root_path.join("dir/index.md"), "DIR").unwrap();

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

#[actix_web::test]
#[traced_test]
async fn test_file_page_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.root_path.join("page.md"), "PAGE").unwrap();

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

#[actix_web::test]
#[traced_test]
async fn test_static_file_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.root_path.join("a.txt"), "AAA").unwrap();

    assert!(
        Response::static_other("AAA", mime::TEXT_PLAIN_UTF_8)
            == get(&app, "/a.txt").await
    );
    assert!(Response::redirect("/a.txt") == get(&app, "/a.txt/").await);
    assert!(Response::redirect("/a.txt") == get(&app, "/a.txt/.").await);
    assert!(Response::redirect("/a.txt") == get(&app, "/a.txt/././").await);
}

#[actix_web::test]
#[traced_test]
async fn test_static_directory_get() {
    let (_dir, config, app) = init_app().await;

    fs::create_dir(config.root_path.join("b")).unwrap();
    fs::write(config.root_path.join("b/index.html"), "BBB").unwrap();

    assert!(Response::static_html("BBB") == get(&app, "/b/").await);
    assert!(Response::redirect("/b/") == get(&app, "/b").await);
    assert!(Response::redirect("/b/") == get(&app, "/b/index.html").await);
    assert!(Response::redirect("/b/") == get(&app, "/b/././index.html").await);
}

#[actix_web::test]
#[traced_test]
async fn test_static_index_with_page() {
    let (_dir, config, app) = init_app().await;

    fs::create_dir(config.root_path.join("static")).unwrap();
    fs::write(config.root_path.join("static/index.html"), "STATIC").unwrap();
    fs::write(config.root_path.join("static/page.md"), "PAGE").unwrap();

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

#[actix_web::test]
#[traced_test]
async fn test_static_hides_page() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.root_path.join("index.html"), "STATIC").unwrap();
    fs::write(config.root_path.join("index.md"), "PAGE").unwrap();

    assert!(Response::static_html("STATIC") == get(&app, "/").await);
    assert!(Response::redirect("/") == get(&app, "/index.html").await);
    assert!(Response::page_source("PAGE") == get(&app, "/index.md").await);
}

#[actix_web::test]
#[traced_test]
async fn test_not_found_get() {
    let (_dir, _config, app) = init_app().await;

    assert!(
        Response {
            status: http::StatusCode::NOT_FOUND,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: B(b"404"),
        } == get(&app, "/not-found").await
    );
}

#[cfg(all(not(target_os = "hermit"), unix))]
#[actix_web::test]
#[traced_test]
async fn test_forbidden_page_get() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, config, app) = init_app().await;

    let path = config.root_path.join("forbidden.md");
    fs::write(&path, "forbidden").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

    assert!(
        Response {
            status: http::StatusCode::FORBIDDEN,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: B(b"403"),
        } == get(&app, "/forbidden").await
    );

    assert!(
        Response {
            status: http::StatusCode::FORBIDDEN,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: B(b"403"),
        } == get(&app, "/forbidden.md").await
    );
}

#[cfg(all(not(target_os = "hermit"), unix))]
#[actix_web::test]
#[traced_test]
async fn test_forbidden_static_get() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, config, app) = init_app().await;

    let path = config.root_path.join("forbidden.txt");
    fs::write(&path, "forbidden").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

    assert!(
        Response {
            status: http::StatusCode::FORBIDDEN,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
            location: None,
            body: B(b"403"),
        } == get(&app, "/forbidden.txt").await
    );
}
