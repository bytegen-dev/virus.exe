mod agent;
mod memory;
mod ollama;
mod service;
mod shell;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "virus", about = "an autonomous AI agent that lives on your machine")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Install virus.exe as a Windows service (requires admin)
    Install,
    /// Uninstall the virus.exe Windows service (requires admin)
    Uninstall,
    /// Run as Windows service (called by SCM, do not use directly)
    Service,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Install) => {
            eprintln!(
                r#"
  ██╗   ██╗██╗██████╗ ██╗   ██╗███████╗
  ██║   ██║██║██╔══██╗██║   ██║██╔════╝
  ██║   ██║██║██████╔╝██║   ██║███████╗
  ╚██╗ ██╔╝██║██╔══██╗██║   ██║╚════██║
   ╚████╔╝ ██║██║  ██║╚██████╔╝███████║
    ╚═══╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝
            installing service...
"#
            );
            if let Err(e) = service::install_service() {
                eprintln!("[virus] install failed: {}", e);
                eprintln!("[virus] try running as administrator");
                std::process::exit(1);
            }
        }
        Some(Commands::Uninstall) => {
            if let Err(e) = service::uninstall_service() {
                eprintln!("[virus] uninstall failed: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Service) => {
            // called by windows SCM
            if let Err(e) = service::dispatch() {
                eprintln!("[virus-service] dispatch failed: {}", e);
            }
        }
        None => {
            // foreground mode
            eprintln!(
                r#"
  ██╗   ██╗██╗██████╗ ██╗   ██╗███████╗
  ██║   ██║██║██╔══██╗██║   ██║██╔════╝
  ██║   ██║██║██████╔╝██║   ██║███████╗
  ╚██╗ ██╔╝██║██╔══██╗██║   ██║╚════██║
   ╚████╔╝ ██║██║  ██║╚██████╔╝███████║
    ╚═══╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝
            
  autonomous agent — running in foreground
  press Ctrl+C to stop

  tip: run 'virus.exe install' to run as a service
"#
            );

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(agent::run(None));
        }
    }
}
