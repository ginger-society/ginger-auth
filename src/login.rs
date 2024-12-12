use crate::workspace::WorkspaceSummaryOption;
use dirs;
use inquire::{validator::MinLengthValidator, Password, PasswordDisplayMode, Text};
use inquire::{InquireError, Select};
use serde_json::json;
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::exit,
};
use IAMService::{
    apis::{
        configuration::Configuration,
        default_api::{
            identity_create_api_session_token_interactive, identity_login,
            IdentityCreateApiSessionTokenInteractiveParams, IdentityLoginParams,
        },
    },
    get_configuration as iam_get_configuration,
    models::LoginRequest,
};
use MetadataService::{
    apis::default_api::metadata_get_workspaces, get_configuration as metadata_get_configuration,
};

pub async fn login(iam_config: Configuration) {
    match Text::new("What's your email ID?")
        .with_validator(MinLengthValidator::new(1))
        .prompt()
    {
        Ok(user_id) => {
            let password = match Password::new("Enter password:")
                .without_confirmation()
                .with_display_mode(PasswordDisplayMode::Masked)
                .prompt()
            {
                Ok(p) => p,
                Err(_) => {
                    println!("You cancelled, can't proceed without the password");
                    exit(1);
                }
            };

            match identity_login(
                &iam_config,
                IdentityLoginParams {
                    login_request: LoginRequest {
                        email: user_id,
                        password,
                        client_id: Some(Some("dev-machine".to_string())), //TODO: should take a parameter to set to pipeline in case we are using it there
                    },
                },
            )
            .await
            {
                Ok(login_response) => {
                    let metadata_config =
                        metadata_get_configuration(Some(login_response.access_token.clone()));
                    let iam_config =
                        iam_get_configuration(Some(login_response.access_token.clone()));

                    match metadata_get_workspaces(&metadata_config).await {
                        Ok(workspaces_response) => {
                            let workspaces_options: Vec<WorkspaceSummaryOption> =
                                workspaces_response
                                    .into_iter()
                                    .map(|ws| WorkspaceSummaryOption {
                                        slug: ws.slug.clone(),
                                        name: ws.name.clone(),
                                        group_id: ws.group_id.clone(),
                                    })
                                    .collect();

                            let ans: Result<WorkspaceSummaryOption, InquireError> =
                                Select::new("Please select the workspace", workspaces_options)
                                    .prompt();

                            match ans {
                                Ok(selected_option) => {
                                    match identity_create_api_session_token_interactive(
                                        &iam_config,
                                        IdentityCreateApiSessionTokenInteractiveParams {
                                            group_identifier: selected_option.group_id,
                                        },
                                    )
                                    .await
                                    {
                                        Ok(token_response) => {
                                            let session_token = token_response.session_token;

                                            // Locate the user's home directory
                                            let home_dir = match dirs::home_dir() {
                                                Some(path) => path,
                                                None => {
                                                    println!(
                                                        "Failed to locate home directory. Exiting."
                                                    );
                                                    exit(1);
                                                }
                                            };

                                            // Construct the path to the auth.json file
                                            let auth_dir_path: PathBuf =
                                                [home_dir.to_str().unwrap(), ".ginger-society"]
                                                    .iter()
                                                    .collect();
                                            let auth_file_path = auth_dir_path.join("auth.json");

                                            // Create the directory if it doesn't exist
                                            if let Err(e) = fs::create_dir_all(&auth_dir_path) {
                                                println!(
                                                    "Failed to create directory {}: {}. Exiting.",
                                                    auth_dir_path.display(),
                                                    e
                                                );
                                                exit(1);
                                            }

                                            // Prepare the JSON content
                                            let json_content = json!({
                                                "API_TOKEN": session_token
                                            });

                                            // Write the token to the file
                                            let mut file = match File::create(&auth_file_path) {
                                                Ok(f) => f,
                                                Err(e) => {
                                                    println!(
                                                        "Failed to create file {}: {}. Exiting.",
                                                        auth_file_path.display(),
                                                        e
                                                    );
                                                    exit(1);
                                                }
                                            };

                                            if let Err(e) =
                                                file.write_all(json_content.to_string().as_bytes())
                                            {
                                                println!(
                                                    "Failed to write to file {}: {}. Exiting.",
                                                    auth_file_path.display(),
                                                    e
                                                );
                                                exit(1);
                                            }

                                            println!(
                                                "API token saved to {}",
                                                auth_file_path.display()
                                            );
                                        }
                                        Err(e) => {
                                            println!("Error creating API session token: {:?}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("Error selecting workspace: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Error getting workspaces: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("An error occurred, please check your credentials and try again. You may try recovering your password if needed.");
                    println!("{:?}", e);
                }
            }
        }
        Err(_) => {
            println!("We cannot proceed without your email ID, please try again if you change your mind.");
            exit(1);
        }
    }
}
