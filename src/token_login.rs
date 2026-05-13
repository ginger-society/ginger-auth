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
        default_api::{
            IdentityCreateApiSessionTokenParams, gitter_ssh_cert_api_land, gitter_ssh_cert_user_land, identity_create_api_session_token
        },
    },
    get_configuration as iam_get_configuration,
    models::CreateSessionTokenRequest,
};


use MetadataService::{
    apis::default_api::metadata_get_current_workspace, get_configuration as metadata_get_configuration
};

use crate::login_helpers::{docker_login, update_npmrc, update_pypirc, write_ssh_file};

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

            // ── locate home dir ───────────────────────────────────────────────
            let home_dir = match dirs::home_dir() {
                Some(path) => path,
                None => {
                    println!("Failed to locate home directory. Exiting.");
                    exit(1);
                }
            };

            // ── save to ~/.ginger-society/auth.json ───────────────────────────
            let auth_dir_path: PathBuf = [home_dir.to_str().unwrap(), ".ginger-society"]
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
                    println!(
                        "Failed to create file {}: {}. Exiting.",
                        auth_file_path.display(),
                        e
                    );
                    exit(1);
                }
            }

            let authed_metadata_config = metadata_get_configuration(Some(session_token.clone()));

            let slug : String= match metadata_get_current_workspace(&authed_metadata_config).await {
                Ok(workspace_response) => {
                    workspace_response.org_id
                }
                Err(e) => {
                    println!("Warning: could not fetch workspace info: {:?}", e);
                    // continue with generic setup...
                    exit(0);
                }
            };

            // ── fetch SSH certificate ─────────────────────────────────────────
            // re-build iam_config with the new session token so the cert
            // endpoint is authenticated correctly
            let authed_iam_config = iam_get_configuration(Some(session_token.clone()));
            match gitter_ssh_cert_api_land(&authed_iam_config).await {
                Ok(cert_response) => {
                    let ssh_dir: PathBuf =
                        [home_dir.to_str().unwrap(), ".ssh"].iter().collect();

                    if let Err(e) = fs::create_dir_all(&ssh_dir) {
                        println!(
                            "Failed to create ~/.ssh: {}. Skipping SSH cert.",
                            e
                        );
                    } else {
                        write_ssh_file(
                            &ssh_dir.join("id_ed25519"),
                            &cert_response.private_key_pem,
                            0o600,
                        );
                        write_ssh_file(
                            &ssh_dir.join("id_ed25519.pub"),
                            &cert_response.public_key,
                            0o644,
                        );
                        write_ssh_file(
                            &ssh_dir.join("id_ed25519-cert.pub"),
                            &cert_response.certificate,
                            0o644,
                        );
                        println!(
                            "SSH certificate written to ~/.ssh/id_ed25519-cert.pub (valid for {})",
                            cert_response.valid_for
                        );
                    }
                }
                Err(e) => {
                    // non-fatal — auth.json is already saved
                    println!("Warning: could not fetch SSH certificate: {:?}", e);
                }
            }

            // ── update ~/.npmrc ───────────────────────────────────────────────
            update_npmrc(&home_dir, &session_token, &slug);

            // ── update ~/.pypirc ──────────────────────────────────────────────
            update_pypirc(&home_dir, &session_token, &slug);

            // ── docker login ──────────────────────────────────────────────────
            docker_login(&session_token);
        }
        Err(e) => {
            println!("{:?}", e);
        }
    }
}

