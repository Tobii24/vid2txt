use anyhow::Result;

mod app;
mod cli;
mod cmd;
mod constants;
mod fs_utils;
mod hf;
mod models;

fn main() -> Result<()> {
    app::run()
}
