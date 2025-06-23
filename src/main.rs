use color_eyre::eyre::Result;
mod app;
mod audio;
mod logging;
use app::App;
use logging::initialize_logging;

fn main() -> Result<()> {
    // todo: maybe do better parsing using clap
    // can also get this input thru the tui itself
    // if we have a nice file picker or at least
    // auto-complete that is file system-aware
    let mut args = std::env::args();
    let _ = args.next(); // ignore binary
    let file = args.next();
    initialize_logging()?;
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::new(file)?.run(terminal);
    ratatui::restore();
    app_result
}
