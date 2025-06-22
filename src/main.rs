use color_eyre::eyre::Result;
mod app;
mod audio;
mod logging;
use app::App;
use logging::initialize_logging;

fn main() -> Result<()> {
    initialize_logging()?;
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::new().run(terminal);
    ratatui::restore();
    app_result
}
