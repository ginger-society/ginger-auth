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
            gitter_ssh_cert_user_land,                          // ← new
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
                        client_id: Some(Some("dev-machine".to_string())),
                    },
                },
            )
            .await
            {
                Ok(login_response) => {
                    let metadata_config = metadata_get_configuration(Some(
                        login_response
                            .app_tokens
                            .clone()
                            .unwrap()
                            .unwrap()
                            .access_token
                            .clone(),
                    ));
                    let iam_config = iam_get_configuration(Some(
                        login_response
                            .app_tokens
                            .clone()
                            .unwrap()
                            .unwrap()
                            .access_token
                            .clone(),
                    ));

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

                                            // ── locate home dir (needed for both paths) ──────
                                            let home_dir = match dirs::home_dir() {
                                                Some(path) => path,
                                                None => {
                                                    println!(
                                                        "Failed to locate home directory. Exiting."
                                                    );
                                                    exit(1);
                                                }
                                            };

                                            // ── save API token to ~/.ginger-society/auth.json ─
                                            let auth_dir_path: PathBuf =
                                                [home_dir.to_str().unwrap(), ".ginger-society"]
                                                    .iter()
                                                    .collect();
                                            let auth_file_path = auth_dir_path.join("auth.json");

                                            if let Err(e) = fs::create_dir_all(&auth_dir_path) {
                                                println!(
                                                    "Failed to create directory {}: {}. Exiting.",
                                                    auth_dir_path.display(),
                                                    e
                                                );
                                                exit(1);
                                            }

                                            let json_content = json!({ "API_TOKEN": session_token });

                                            match File::create(&auth_file_path) {
                                                Ok(mut file) => {
                                                    if let Err(e) = file.write_all(
                                                        json_content.to_string().as_bytes(),
                                                    ) {
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
                                                    println!(
                                                        "Failed to create file {}: {}. Exiting.",
                                                        auth_file_path.display(),
                                                        e
                                                    );
                                                    exit(1);
                                                }
                                            }

                                            // ── fetch SSH certificate (Claims / user-land) ───
                                            match gitter_ssh_cert_user_land(&iam_config).await {
                                                Ok(cert_response) => {
                                                    let ssh_dir: PathBuf =
                                                        [home_dir.to_str().unwrap(), ".ssh"]
                                                            .iter()
                                                            .collect();

                                                    if let Err(e) = fs::create_dir_all(&ssh_dir) {
                                                        println!(
                                                            "Failed to create ~/.ssh: {}. Skipping SSH cert.",
                                                            e
                                                        );
                                                    } else {
                                                        // private key  → ~/.ssh/id_ed25519
                                                        write_ssh_file(
                                                            &ssh_dir.join("id_ed25519"),
                                                            &cert_response.private_key_pem,
                                                            0o600,
                                                        );

                                                        // public key   → ~/.ssh/id_ed25519.pub
                                                        write_ssh_file(
                                                            &ssh_dir.join("id_ed25519.pub"),
                                                            &cert_response.public_key,
                                                            0o644,
                                                        );

                                                        // certificate  → ~/.ssh/id_ed25519-cert.pub
                                                        write_ssh_file(
                                                            &ssh_dir.join("id_ed25519-cert.pub"),
                                                            &cert_response.certificate,
                                                            0o644,
                                                        );

                                                        println!(
                                                            "SSH certificate written to ~/.ssh/id_ed25519-cert.pub \
                                                             (valid for {})",
                                                            cert_response.valid_for
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    // Non-fatal: auth.json is already saved.
                                                    println!(
                                                        "Warning: could not fetch SSH certificate: {:?}",
                                                        e
                                                    );
                                                }
                                            }
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
                    println!(
                        "An error occurred, please check your credentials and try again. \
                         You may try recovering your password if needed."
                    );
                    println!("{:?}", e);
                }
            }
        }
        Err(_) => {
            println!(
                "We cannot proceed without your email ID, please try again if you change your mind."
            );
            exit(1);
        }
    }
}

// ── helper: write a file and set Unix permissions ─────────────────────────────

#[cfg(unix)]
fn write_ssh_file(path: &PathBuf, content: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    match File::create(path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                println!("Failed to write {}: {}", path.display(), e);
                return;
            }
            if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
                println!("Warning: could not set permissions on {}: {}", path.display(), e);
            }
        }
        Err(e) => println!("Failed to create {}: {}", path.display(), e),
    }
}

#[cfg(not(unix))]
fn write_ssh_file(path: &PathBuf, content: &str, _mode: u32) {
    // Windows: just write the file; SSH permission enforcement isn't applicable
    match File::create(path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                println!("Failed to write {}: {}", path.display(), e);
            }
        }
        Err(e) => println!("Failed to create {}: {}", path.display(), e),
    }
}