//! Benchmark requests to the default rules.
#![allow(
    clippy::missing_docs_in_private_items,
    missing_docs,
    reason = "benchmarks"
)]

use axum::body::Body;
use axum::extract::Request;
use criterion::{Criterion, criterion_group, criterion_main};
use riki::httpd::init_test_app;
use std::fs;
use std::time::Duration;
use tokio::runtime::Runtime;
use tower::Service;

fn benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("default_run");
    group
        .noise_threshold(0.10)
        .significance_level(0.01)
        .confidence_level(0.99)
        .sample_size(300)
        .warm_up_time(Duration::from_millis(10))
        .measurement_time(Duration::from_millis(100));

    let (_dir, root, mut app) = init_test_app();

    fs::write(root.join("index.md"), "index").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/index.md"), "DIR").unwrap();

    let runtime = Runtime::new().unwrap();
    let service = runtime
        .block_on(async {
            <axum::Router as tower::ServiceExt<Request>>::ready(&mut app).await
        })
        .unwrap();

    group.bench_function("index", |b| {
        b.to_async(&runtime).iter(|| service.call(get("/")));
    });

    group.bench_function("index_source", |b| {
        b.to_async(&runtime).iter(|| service.call(get("/index.md")));
    });

    group.bench_function("dir_redirect", |b| {
        b.to_async(&runtime).iter(|| service.call(get("/dir")));
    });

    group.finish();
}

/// Create a GET request.
///
/// # Panics
///
/// Panics if it couldn’t create the GET request.
fn get(uri: &str) -> Request {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

criterion_group!(benchmark_group, benchmarks);
criterion_main!(benchmark_group);
