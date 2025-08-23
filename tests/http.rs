//! Test HTTP server.

use actix_http::Request;
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::test::TestRequest;
use actix_web::web::Data;
use actix_web::{App, body, test, web};
use assert2::{check, let_assert};
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

/// Convert byte string to right type for comparison with a response body.
const B: fn(&'static [u8]) -> web::Bytes = web::Bytes::from_static;

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

    // FIXME check content-type
    let resp = get(&app, "/").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"<p>index</p>\n"));

    let resp = get(&app, "/.").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"<p>index</p>\n"));
}

#[actix_web::test]
#[traced_test]
async fn test_static_get() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.static_path.join("a.txt"), "AAA").unwrap();

    // FIXME check content-type
    let resp = get(&app, "/a.txt").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"AAA"));

    let resp = get(&app, "/a.txt/").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"AAA"));

    let resp = get(&app, "/a.txt/.").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"AAA"));
}

#[actix_web::test]
#[traced_test]
async fn test_fall_through() {
    let (_dir, config, app) = init_app().await;

    fs::write(config.static_path.join("static"), "STATIC").unwrap();
    fs::create_dir(config.pages_path.join("static")).unwrap();
    fs::write(config.pages_path.join("static/page.md"), "PAGE").unwrap();

    // FIXME check content-type
    let resp = get(&app, "/static").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"STATIC"));

    let resp = get(&app, "/static/page").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"<p>PAGE</p>\n"));

    let resp = get(&app, "/static/page/.").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"<p>PAGE</p>\n"));
}

#[actix_web::test]
#[traced_test]
async fn test_static_index_get() {
    let (_dir, config, app) = init_app().await;

    let b_dir = config.static_path.join("b");
    fs::create_dir(b_dir).unwrap();
    fs::write(config.static_path.join("b/index.html"), "BBB").unwrap();

    // FIXME check content-type
    let resp = get(&app, "/b").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"BBB"));

    let resp = get(&app, "/b/").await;
    check!(resp.status().as_u16() == 200);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"BBB"));
}

#[actix_web::test]
#[traced_test]
async fn test_not_found_get() {
    let (_dir, _config, app) = init_app().await;

    let resp = get(&app, "/not-found").await;
    check!(resp.status().as_u16() == 404);
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"404"));
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
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"403"));
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
    let_assert!(Ok(body) = body::to_bytes(resp.into_body()).await);
    check!(body == B(b"403"));
}
