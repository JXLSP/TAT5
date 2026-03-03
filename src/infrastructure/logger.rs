use crate::infrastructure::config::CFG;
use jiff::{Zoned, fmt::strtime};
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::FormatTime;

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = Zoned::now();
        write!(
            w,
            "{}",
            strtime::format("%Y-%m-%d %H-%M:%S.3f", &now).unwrap_or_default()
        )
    }
}

pub fn init() -> WorkerGuard {
    let (level, (non_blocking, guard)) = match CFG.get() {
        Some(cfg) => {
            let level = if cfg.get_bool("app.debug").unwrap_or_default() {
                Level::DEBUG
            } else {
                Level::INFO
            };
            let appender = if cfg.get_string("app.env").unwrap_or("dev".to_string()) == "dev" {
                tracing_appender::non_blocking(std::io::stdout())
            } else {
                tracing_appender::non_blocking(tracing_appender::rolling::daily(
                    cfg.get_string("log.path").unwrap_or(String::from("logs")),
                    cfg.get_string("log.filename")
                        .unwrap_or(String::from("tracing.log")),
                ))
            };
            (level, appender)
        }
        None => (
            Level::DEBUG,
            tracing_appender::non_blocking(tracing_appender::rolling::daily("logs", "tat5.log")),
        ),
    };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_timer(LocalTimer)
        .with_writer(non_blocking)
        .json()
        .flatten_event(true)
        .init();

    guard
}
