#![allow(clippy::result_large_err)]

mod adapter;
mod brain;
mod cli;
mod doctor;
mod embedded_skills;
mod init;
mod model;
mod output;
mod plan;
mod proxy;
mod research;
mod rtk;
mod run;
mod session;
mod setup;
mod skills;
mod verify;

use clap::Parser;
use orquestra_core::config::{init_tracing, load_config};
use orquestra_core::error::OrquestraError;
use std::process::ExitCode;

fn run() -> Result<(), OrquestraError> {
    let cli = cli::Cli::parse();
    init_tracing(&cli.log_level);

    let project_dir = std::env::current_dir().ok();
    let config = load_config(
        Some(cli.output.clone()),
        Some(cli.log_level.clone()),
        project_dir,
    )?;

    match &cli.command {
        cli::Command::Doctor { security } => {
            let data = doctor::run(&config, *security)?;
            output::print_output(&data, &config.output);
            Ok(())
        }
        cli::Command::Adapter { action } => adapter::run(action, &config.output),
        cli::Command::Skill { action } => skills::handle_skills(action, &config.output),
        cli::Command::Brain { action } => brain::run(action, &config.output),
        cli::Command::Proxy { host, args } => proxy::run(host, args, &config, &config.output),
        cli::Command::Verify { action } => verify::run(action, &config.output),
        cli::Command::Model { action } => model::run(action, &config.output),
        cli::Command::Research { action } => research::run(action, &config.output),
        cli::Command::Plan { action } => plan::run(action, &config.output),
        cli::Command::Run { action } => run::run(action, &config.output),
        cli::Command::Session { action } => session::run(action, &config.output),
        cli::Command::Init { action } => init::run(action, &config),
        cli::Command::Setup { host, dry_run } => setup::run(host, *dry_run, &config.output),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code() as u8)
        }
    }
}
