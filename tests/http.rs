//! Test HTTP server.

use actix_http::{Request, header};
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::http::header::HeaderName;
use actix_web::test::TestRequest;
use actix_web::web::{Bytes, Data};
use actix_web::{App, body, test};
use assert2::check;
use riki::TemplateManager;
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

    let mut tpls = TemplateManager::new();
    tpls.load_from_string("default", "{{& body }}").unwrap();
    tpls.load_from_string("error403", "403").unwrap();
    tpls.load_from_string("error404", "404").unwrap();
    tpls.load_from_string("error500", "{{& error_debug }}")
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

/// Make a GET request to the test app.
#[expect(clippy::future_not_send)] // Actix doesn’t require Send.
async fn get<S, B, E>(app: S, uri: &str) -> S::Response
where
    S: Service<Request, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
{
    test::call_service(&app, TestRequest::get().uri(uri).to_request()).await
}

#[actix_web::test]
#[traced_test]
async fn test_index_page_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.pages_path.join("index.md"), "index").unwrap();

    let resp = get(&app, "/").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED) == None);
    check!(get_header(&resp, header::ETAG) == None);
    check!(get_body(resp).await == B(b"<p>index</p>\n"));

    let resp = get(&app, "/.").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED) == None);
    check!(get_header(&resp, header::ETAG) == None);
    check!(get_body(resp).await == B(b"<p>index</p>\n"));
}

#[actix_web::test]
#[traced_test]
async fn test_static_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.static_path.join("a.txt"), "AAA").unwrap();

    let resp = get(&app, "/a.txt").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_PLAIN_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED).is_some());
    check!(get_header(&resp, header::ETAG).is_some());
    check!(get_body(resp).await == B(b"AAA"));

    let resp = get(&app, "/a.txt/").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_PLAIN_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED).is_some());
    check!(get_header(&resp, header::ETAG).is_some());
    check!(get_body(resp).await == B(b"AAA"));

    let resp = get(&app, "/a.txt/.").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_PLAIN_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED).is_some());
    check!(get_header(&resp, header::ETAG).is_some());
    check!(get_body(resp).await == B(b"AAA"));
}

#[actix_web::test]
#[traced_test]
async fn test_static_index_get() {
    let (_dir, config, app) = init_app().await;

    let b_dir = config.static_path.join("b");
    fs::create_dir(b_dir).unwrap();
    fs::write(config.static_path.join("b/index.html"), "BBB").unwrap();

    let resp = get(&app, "/b").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED).is_some());
    check!(get_header(&resp, header::ETAG).is_some());
    check!(get_body(resp).await == B(b"BBB"));

    let resp = get(&app, "/b/").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED).is_some());
    check!(get_header(&resp, header::ETAG).is_some());
    check!(get_body(resp).await == B(b"BBB"));
}

#[actix_web::test]
#[traced_test]
async fn test_fall_through() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.static_path.join("static"), "STATIC").unwrap();
    fs::create_dir(config.pages_path.join("static")).unwrap();
    fs::write(config.pages_path.join("static/page.md"), "PAGE").unwrap();

    let resp = get(&app, "/static").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::APPLICATION_OCTET_STREAM));
    check!(get_body(resp).await == B(b"STATIC"));

    let resp = get(&app, "/static/page").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_body(resp).await == B(b"<p>PAGE</p>\n"));

    let resp = get(&app, "/static/page/.").await;
    check!(resp.status().as_u16() == 200);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_body(resp).await == B(b"<p>PAGE</p>\n"));
}

#[actix_web::test]
#[traced_test]
async fn test_not_found_get() {
    let (_dir, _config, app) = init_app().await;

    let resp = get(&app, "/not-found").await;
    check!(resp.status().as_u16() == 404);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED) == None);
    check!(get_header(&resp, header::ETAG) == None);
    check!(get_body(resp).await == B(b"404"));
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

    let resp = get(&app, "/forbidden").await;
    check!(resp.status().as_u16() == 403);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED) == None);
    check!(get_header(&resp, header::ETAG) == None);
    check!(get_body(resp).await == B(b"403"));
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

    let resp = get(&app, "/forbidden.txt").await;
    check!(resp.status().as_u16() == 403);
    check!(get_content_type(&resp) == Some(mime::TEXT_HTML_UTF_8));
    check!(get_header(&resp, header::LAST_MODIFIED) == None);
    check!(get_header(&resp, header::ETAG) == None);
    check!(get_body(resp).await == B(b"403"));
}
