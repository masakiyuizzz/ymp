mod app;
mod audio;
mod cli;
mod config;
mod graphics;
mod i18n;
mod model;
mod scan;
mod spectrum;
mod selftest;
mod term;
mod theme;
mod ui;

fn main() {
    let args = cli::parse();

    if args.render {
        let w = std::env::var("YMP_RENDER_W").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        let h = std::env::var("YMP_RENDER_H").ok().and_then(|v| v.parse().ok()).unwrap_or(34);
        if let Err(e) = selftest::render(&args, w, h) {
            eprintln!("ymp render: {e}");
            std::process::exit(1);
        }
        return;
    }

    if args.selftest {
        if let Err(e) = selftest::run(&args) {
            eprintln!("ymp selftest: {e}");
            std::process::exit(1);
        }
        return;
    }

    let caps = term::probe();

    if args.probe {
        term::print_caps(&caps);
        return;
    }

    if let Err(e) = term::enter() {
        eprintln!("ymp: cannot initialise terminal: {e}");
        std::process::exit(1);
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = term::leave();
        default_hook(info);
    }));

    let mut app = app::App::new(&args, caps);
    let result = app.run();
    let _ = term::leave();

    if let Err(e) = result {
        eprintln!("ymp: {e}");
        std::process::exit(1);
    }
}
