//! Test HTTP server.

use actix_http::{Request, header};
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::http::header::HeaderName;
use actix_web::test::TestRequest;
use actix_web::web::{Bytes, Data};
use actix_web::{App, body, http, test};
use assert2::check;
use riki::http::{Configuration, path_handler};
use std::fs;
use temp_dir::TempDir;
use tracing_test::traced_test;

/// Initialize the test app.
#[expect(clippy::future_not_send)] // Actix doesn’t require Send.
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
    fs::create_dir(&config.pages_path).unwrap();
    fs::create_dir(&config.static_path).unwrap();

    let mut tpls = riki::templates();
    tpls.clear_templates();
    tpls.register_template_string("default", "{{{ body }}}")
        .unwrap();
    tpls.register_template_string("error403", "403").unwrap();
    tpls.register_template_string("error404", "404").unwrap();
    tpls.register_template_string("error500", "{{{ error_debug }}}")
        .unwrap();
    tpls.register_template_string(
        "redirect301",
        "redirect {{ canonical_url }}",
    )
    .unwrap();

    (
        temp_dir,
        config.clone(),
        test::init_service(
            App::new()
                .app_data(Data::new(tpls))
                .app_data(Data::new(config))
                .service(path_handler),
        )
        .await,
    )
}

/// Get body of response
#[expect(clippy::future_not_send)] // Actix doesn’t require Send.
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
    body: Bytes,
}

impl Response {
    /// Create the summary from an actual [`ServiceResponse`].
    #[expect(clippy::future_not_send)] // Actix doesn’t require Send.
    async fn from(resp: ServiceResponse<BoxBody>) -> Self {
        Self {
            status: resp.status(),
            content_type: get_content_type(&resp),
            last_modified: get_header(&resp, header::LAST_MODIFIED).is_some(),
            etag: get_header(&resp, header::ETAG).is_some(),
            body: get_body(resp).await,
        }
    }

    /// An expected 301 Moved Permanently response.
    fn redirect(to: &str) -> Self {
        // FIXME add Location header
        Self {
            status: http::StatusCode::MOVED_PERMANENTLY,
            content_type: Some(mime::TEXT_HTML_UTF_8),
            last_modified: false,
            etag: false,
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
            body: body.to_owned().into(),
        }
    }

    /// An expected response for static HTML file.
    fn static_html(body: &str) -> Self {
        Self::static_other(body, mime::TEXT_HTML_UTF_8)
    }

    /// An expected response for static file of `content_type`.
    fn static_other(body: &str, content_type: mime::Mime) -> Self {
        Self {
            status: http::StatusCode::OK,
            content_type: Some(content_type),
            last_modified: true,
            etag: true,
            body: body.to_owned().into(),
        }
    }
}

/// Make a GET request to the test app.
#[expect(clippy::future_not_send)] // Actix doesn’t require Send.
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
async fn test_index_page_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.pages_path.join("index.md"), "index").unwrap();

    check!(Response::page_html("<p>index</p>\n") == get(&app, "/").await);
    check!(Response::page_html("<p>index</p>\n") == get(&app, "/.").await);
}

#[actix_web::test]
#[traced_test]
async fn test_static_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.static_path.join("a.txt"), "AAA").unwrap();

    check!(
        Response::static_other("AAA", mime::TEXT_PLAIN_UTF_8)
            == get(&app, "/a.txt").await
    );
    check!(Response::redirect("/a.txt") == get(&app, "/a.txt/").await);
    check!(Response::redirect("/a.txt") == get(&app, "/a.txt/.").await);
}

#[actix_web::test]
#[traced_test]
async fn test_static_index_get() {
    let (_dir, config, app) = init_app().await;

    let b_dir = config.static_path.join("b");
    fs::create_dir(b_dir).unwrap();
    fs::write(config.static_path.join("b/index.html"), "BBB").unwrap();

    check!(Response::static_html("BBB") == get(&app, "/b/").await);
    check!(Response::redirect("/b/") == get(&app, "/b").await);
    check!(Response::redirect("/b/") == get(&app, "/b/index.html").await);
}

#[actix_web::test]
#[traced_test]
async fn test_fall_through() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.static_path.join("static"), "STATIC").unwrap();
    fs::create_dir(config.pages_path.join("static")).unwrap();
    fs::write(config.pages_path.join("static/page.md"), "PAGE").unwrap();

    check!(
        Response::static_other("STATIC", mime::APPLICATION_OCTET_STREAM)
            == get(&app, "/static").await
    );
    check!(Response::redirect("/static") == get(&app, "/static/").await);
    check!(
        Response::page_html("<p>PAGE</p>\n") == get(&app, "/static/page").await
    );
    check!(
        Response::page_html("<p>PAGE</p>\n")
            == get(&app, "/static/page/").await
    );
}

#[actix_web::test]
#[traced_test]
async fn test_not_found_get() {
    let (_dir, _config, app) = init_app().await;

    let received = get(&app, "/not-found").await;
    let expected = Response {
        status: http::StatusCode::NOT_FOUND,
        content_type: Some(mime::TEXT_HTML_UTF_8),
        last_modified: false,
        etag: false,
        body: B(b"404"),
    };
    check!(expected == received);
}

#[cfg(all(not(target_os = "hermit"), unix))]
#[actix_web::test]
#[traced_test]
async fn test_forbidden_page_get() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, config, app) = init_app().await;

    let path = config.pages_path.join("forbidden.md");
    fs::write(&path, "forbidden").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

    let received = get(&app, "/forbidden").await;
    let expected = Response {
        status: http::StatusCode::FORBIDDEN,
        content_type: Some(mime::TEXT_HTML_UTF_8),
        last_modified: false,
        etag: false,
        body: B(b"403"),
    };
    check!(expected == received);
}

#[cfg(all(not(target_os = "hermit"), unix))]
#[actix_web::test]
#[traced_test]
async fn test_forbidden_static_get() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, config, app) = init_app().await;

    let path = config.static_path.join("forbidden.txt");
    fs::write(&path, "forbidden").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();

    let received = get(&app, "/forbidden.txt").await;
    let expected = Response {
        status: http::StatusCode::FORBIDDEN,
        content_type: Some(mime::TEXT_HTML_UTF_8),
        last_modified: false,
        etag: false,
        body: B(b"403"),
    };
    check!(expected == received);
}
