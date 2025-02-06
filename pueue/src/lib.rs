// This lint is generating way too many false-positives.
// Ignore it for now.
#![allow(clippy::assigning_clones)]
#![doc = include_str!("../README.md")]

pub(crate) mod prelude {
    #[allow(unused_imports)]
    pub(crate) use tracing::{debug, error, info, trace, warn};

    pub(crate) use crate::errors::*;
}

pub(crate) mod errors {
    pub use color_eyre::eyre::{bail, WrapErr};
    pub use color_eyre::Result;
}

pub mod tracing {
    use crate::prelude::*;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::{
        fmt::time::OffsetTime, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
    };

    pub fn install_tracing(verbosity: u8) -> Result<()> {
        let mut pretty = false;
        let level = match verbosity {
            0 => LevelFilter::WARN,
            1 => LevelFilter::INFO,
            2 => LevelFilter::DEBUG,
            3 => LevelFilter::TRACE,
            _ => {
                pretty = true;
                LevelFilter::TRACE
            }
        };

        // todo: only log error and continue instead of panicing?
        let offset =
            time::UtcOffset::current_local_offset().wrap_err("should get local offset!")?;
        let timer = OffsetTime::new(
            offset,
            time::macros::format_description!("[hour]:[minute]:[second]"),
        );

        type GenericLayer<S> = Box<dyn Layer<S> + Send + Sync>;
        let fmt_layer: GenericLayer<_> = match pretty {
            false => Box::new(tracing_subscriber::fmt::layer().with_timer(timer).with_writer(std::io::stderr)),
            true => Box::new(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_timer(timer)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_thread_names(true)
                    .with_level(true)
                    .with_ansi(true)
                    .with_span_events(tracing_subscriber::fmt::format::FmtSpan::ACTIVE)
                    .with_writer(std::io::stderr),
            ),
        };
        let filter_layer = EnvFilter::builder()
            .with_default_directive(level.into())
            .from_env()
            .wrap_err("RUST_LOG env variable is invalid")?;

        tracing_subscriber::Registry::default()
            .with(fmt_layer.with_filter(filter_layer))
            .with(tracing_error::ErrorLayer::default())
            .init();

        Ok(())
    }
}

pub mod client;
pub mod daemon;
