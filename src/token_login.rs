use dirs;
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
        default_api::{identity_create_api_session_token, IdentityCreateApiSessionTokenParams},
    },
    models::CreateSessionTokenRequest,
};

pub async fn get_session_token(iam_config: Configuration, token_value: String) {
    match identity_create_api_session_token(
        &iam_config,
        IdentityCreateApiSessionTokenParams {
            create_session_token_request: CreateSessionTokenRequest {
                api_token: token_value,
            },
        },
    )
    .await
    {
        Ok(token_response) => {
            let session_token = token_response.session_token;
            println!("export GINGER_API_TOKEN={}", session_token);

            // Locate the user's home directory
            let home_dir = match dirs::home_dir() {
                Some(path) => path,
                None => {
                    println!("Failed to locate home directory. Exiting.");
                    exit(1);
                }
            };

            // Construct the path to the auth.json file
            let auth_dir_path: PathBuf = [home_dir.to_str().unwrap(), ".ginger-society"]
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

            if let Err(e) = file.write_all(json_content.to_string().as_bytes()) {
                println!(
                    "Failed to write to file {}: {}. Exiting.",
                    auth_file_path.display(),
                    e
                );
                exit(1);
            }

            println!("API token saved to {}", auth_file_path.display());
        }
        Err(e) => {
            println!("{:?}", e);
        }
    }
}
