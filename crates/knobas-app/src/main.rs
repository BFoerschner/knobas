// The Windows release build must not open a console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A pipeline step's child (#2314) is this binary too: it does its step
    // and exits before anything of the app starts.
    if let Some(code) = knobas_app::file_pipeline::main_if_asked() {
        std::process::exit(code);
    }
    knobas_app::run();
}
