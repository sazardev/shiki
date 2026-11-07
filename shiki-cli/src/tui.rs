use anyhow::Result;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use shiki_config::Config;
use shiki_core::NotebookStore;
use shiki_tui::App;

pub fn launch(config: Config, store: NotebookStore) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config, store)?;
    // Ask the terminal for its own fg/bg (OSC 10/11) once, after raw mode is
    // on — the `default` theme's `"auto"` selection derives its 20%-alpha
    // highlight band from those real colors, and `theme.name = "auto"` picks
    // its dark/light half from the background. `app::run` re-queries while
    // idle so changing the terminal/OS scheme re-themes the TUI live.
    // Unsupported terminals return `None` and both features fall back.
    app.set_terminal_colors(shiki_tui::term_colors::query_fg_bg());
    let result = shiki_tui::app::run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result.map_err(anyhow::Error::from)
}
