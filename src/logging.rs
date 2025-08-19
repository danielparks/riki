//! Various logging functions.

use anyhow::bail;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*;

/// Initialize logging for the executable.
pub fn init(verbose: u8) -> anyhow::Result<()> {
    let filter = match verbose {
        4.. => bail!("-v is only allowed up to 3 times."),
        3 => LevelFilter::TRACE,
        2 => LevelFilter::DEBUG,
        1 => LevelFilter::INFO,
        0 => LevelFilter::WARN,
    };

    let formatter = tracing_subscriber::fmt::layer()
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339());
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(filter).with(formatter),
    )?;

    Ok(())
}
