use crate::register::register;
use clap::{Parser, Subcommand};
use info::get_session_info;
use IAMService::{
    apis::default_api::identity_create_api_session_token, get_configuration,
    get_configuration_without_auth,
};
mod info;
mod login;
mod register;
mod token_login;
mod workspace;
/// Command line interface for managing device session
#[derive(Parser)]
#[clap(name = "CLI")]
#[clap(about = "A CLI to manage device session", long_about = None)]
struct CLI {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new account
    Register,
    /// Login with username and password
    Login,
    /// Create a temperary session using token
    TokenLogin {
        #[clap(value_parser)]
        token_value: String,
    },
    Info,
}

#[tokio::main]
async fn main() {
    let cli = CLI::parse();
    let iam_config = get_configuration_without_auth();

    match cli.command {
        Commands::Register => {
            register(iam_config).await;
        }
        Commands::TokenLogin { token_value } => {
            token_login::get_session_token(iam_config, token_value).await;
        }
        Commands::Login => {
            login::login(iam_config).await;
        }
        Commands::Info => {
            get_session_info().await;
        }
    }
}
