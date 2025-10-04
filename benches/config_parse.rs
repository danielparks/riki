//! Benchmark config parser.
#![allow(
    clippy::missing_docs_in_private_items,
    missing_docs,
    reason = "benchmarks"
)]

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use riki::config::parser2::parse;
use std::time::Duration;

const COMPLEX_CONF: &str = include_str!("complex.conf");

fn benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("mime_parse");
    group
        .noise_threshold(0.10)
        .significance_level(0.01)
        .confidence_level(0.99)
        .sample_size(300)
        .warm_up_time(Duration::from_millis(10))
        .measurement_time(Duration::from_millis(100));

    group.throughput(Throughput::Bytes(COMPLEX_CONF.len().try_into().unwrap()));
    group.bench_with_input("complex_conf", COMPLEX_CONF, |b, input| {
        b.iter(|| parse(input).unwrap());
    });

    group.finish();
}

criterion_group!(benchmark_group, benchmarks);
criterion_main!(benchmark_group);
