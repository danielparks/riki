//! Template helpers.

use handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, Output,
    RenderContext, RenderErrorReason, handlebars_helper,
};
use jiff::{Timestamp, tz::TimeZone};

/// `{{$ var}}` — output the value of `var` or nothing if `var` doesn’t exist.
#[derive(Clone, Copy)]
pub struct DollarHelper;

impl HelperDef for DollarHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'rc>,
        _registry: &'reg Handlebars<'reg>,
        _ctx: &'rc Context,
        _render_ctx: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let param = helper
            .param(0)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("$", 0))?;

        let value = param.value();
        if !value.is_null() {
            out.write(
                value
                    .as_str()
                    .ok_or(RenderErrorReason::InvalidParamType("String"))?,
            )?;
        }
        Ok(())
    }
}

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
    tpls.register_helper("$", Box::new(DollarHelper));
}
