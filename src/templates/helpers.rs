//! Template helpers

use handlebars::{Handlebars, RenderErrorReason, handlebars_helper};
use jiff::{Timestamp, tz::TimeZone};

// Format [`Timestamp`], e.g. `source.File.modified`.
handlebars_helper!(strftime:
|time: Timestamp, format: str, { tz: str = "system" }| {
    time.to_zoned(if tz == "system" {
        TimeZone::system()
    } else {
        TimeZone::get(tz).map_err(|error| {
            RenderErrorReason::Other(format!("strftime helper: {error}"))
        })?
    }).strftime(format).to_string()
});

/// Register helpers.
pub fn register(tpls: &mut Handlebars) {
    tpls.register_helper("strftime", Box::new(strftime));
}
