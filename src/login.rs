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

                                            // ── update ~/.npmrc ──────────────────────────────
                                            update_npmrc(&home_dir, &session_token, &selected_option.slug);

                                            // ── update ~/.pypirc ─────────────────────────────
                                            update_pypirc(&home_dir, &session_token, &selected_option.slug);

                                            // ── docker login to docker.gingersociety.org ───────────────────────────────
                                            docker_login(&session_token);
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

// ── ~/.npmrc ──────────────────────────────────────────────────────────────────
// Manages two lines:
//   //npm.gingersociety.org/:_authToken=<token>
//   //@<slug>:registry=https://npm.gingersociety.org/
//
// If the file already exists the relevant lines are updated in-place;
// unrelated lines (other registries, other tokens) are left untouched.

fn update_npmrc(home_dir: &PathBuf, token: &str, slug: &str) {
    let path = home_dir.join(".npmrc");

    let auth_line    = format!("//npm.gingersociety.org/:_authToken={}", token);
    let registry_line = format!("//@{}:registry=https://npm.gingersociety.org/", slug);

    // Read existing content (or start empty)
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();

    // Update or append the auth-token line
    upsert_line(
        &mut lines,
        "//npm.gingersociety.org/:_authToken=",
        &auth_line,
    );

    // Update or append the registry line for this slug.
    // Key prefix includes the slug so different slugs get independent lines.
    upsert_line(
        &mut lines,
        &format!("//@{}:registry=", slug),
        &registry_line,
    );

    let content = lines.join("\n") + "\n";
    match File::create(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                println!("Warning: could not write ~/.npmrc: {}", e);
            } else {
                println!("~/.npmrc updated (registry: @{})", slug);
            }
        }
        Err(e) => println!("Warning: could not open ~/.npmrc: {}", e),
    }
}

// ── ~/.pypirc ─────────────────────────────────────────────────────────────────
// Manages the [ginger-society] section.  The [distutils] index-servers list and
// the [pypi] section are preserved if already present; if they are absent they
// are written with sensible defaults so the file is always valid.

fn update_pypirc(home_dir: &PathBuf, token: &str, _slug: &str) {
    let path = home_dir.join(".pypirc");

    let existing = fs::read_to_string(&path).unwrap_or_default();

    // ── parse into named sections ─────────────────────────────────────────────
    // We represent the file as an ordered list of (header, body_lines) pairs.
    // The synthetic header "" captures any leading lines before the first [].
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_header = String::new();
    let mut current_body: Vec<String> = Vec::new();

    for line in existing.lines() {
        if line.trim_start().starts_with('[') && line.trim_end().ends_with(']') {
            sections.push((current_header.clone(), current_body.clone()));
            current_header = line.trim().to_string();
            current_body = Vec::new();
        } else {
            current_body.push(line.to_string());
        }
    }
    sections.push((current_header, current_body));


    // ── ensure [distutils] lists both pypi and ginger-society ─────────────────
    if let Some((_, body)) = sections.iter_mut().find(|(h, _)| h == "[distutils]") {
        if let Some(pos) = body.iter().position(|l| l.trim_start().starts_with("index-servers")) {
            let current = body[pos].clone();
            if !current.contains("ginger-society") {
                body[pos] = format!("{}\n    ginger-society", current.trim_end());
            }
        } else {
            body.push("index-servers =".to_string());
            body.push("    pypi".to_string());
            body.push("    ginger-society".to_string());
        }
    } else {
        sections.insert(
            1,
            (
                "[distutils]".to_string(),
                vec![
                    "index-servers =".to_string(),
                    "    pypi".to_string(),
                    "    ginger-society".to_string(),
                ],
            ),
        );
    }

    // ── ensure [pypi] section exists ──────────────────────────────────────────
    if !sections.iter().any(|(h, _)| h == "[pypi]") {
        sections.push((
            "[pypi]".to_string(),
            vec![
                "repository = https://upload.pypi.org/legacy/".to_string(),
                "username = __token__".to_string(),
                "password = ".to_string(),
            ],
        ));
    }

    // ── upsert [ginger-society] with fresh token ──────────────────────────────
    let gs_body = vec![
        "repository = https://pip.gingersociety.org".to_string(),
        "username = __token__".to_string(),
        format!("password = {}", token),
    ];

    if let Some((_, body)) = sections.iter_mut().find(|(h, _)| h == "[ginger-society]") {
        *body = gs_body;
    } else {
        sections.push(("[ginger-society]".to_string(), gs_body));
    }

    // ── serialise back ────────────────────────────────────────────────────────
    let mut out = String::new();
    for (header, body) in &sections {
        if !header.is_empty() {
            out.push_str(header);
            out.push('\n');
        }
        for line in body {
            out.push_str(line);
            out.push('\n');
        }
        if !header.is_empty() {
            out.push('\n'); // blank line between sections
        }
    }

    match File::create(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(out.as_bytes()) {
                println!("Warning: could not write ~/.pypirc: {}", e);
            } else {
                println!("~/.pypirc updated ([ginger-society] section)");
            }
        }
        Err(e) => println!("Warning: could not open ~/.pypirc: {}", e),
    }
}

// ── shared line-upsert helper for key=value style files ───────────────────────
// Finds the first line whose prefix matches `key_prefix` and replaces it with
// `new_line`.  If no match is found, appends `new_line`.

fn upsert_line(lines: &mut Vec<String>, key_prefix: &str, new_line: &str) {
    if let Some(pos) = lines.iter().position(|l| l.starts_with(key_prefix)) {
        lines[pos] = new_line.to_string();
    } else {
        lines.push(new_line.to_string());
    }
}


// ── docker login ──────────────────────────────────────────────────────────────
// Runs: echo <token> | docker login registry.gingersociety.org -u __token__ --password-stdin
// Non-fatal: if docker isn't installed or the registry is unreachable we just warn.

fn docker_login(token: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = match Command::new("docker")
        .args(["login", "docker.gingersociety.org", "-u", "__token__", "--password-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("Warning: could not launch docker: {}", e);
            return;
        }
    };

    // Write the token to stdin then close it so docker reads EOF
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(token.as_bytes()) {
            println!("Warning: could not write to docker stdin: {}", e);
            return;
        }
        // stdin dropped here → EOF sent to docker
    }

    match child.wait_with_output() {
        Ok(output) if output.status.success() => {
            println!("Docker login succeeded (docker.gingersociety.org)");
        }
        Ok(output) => {
            println!(
                "Warning: docker login failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(e) => {
            println!("Warning: docker login error: {}", e);
        }
    }
}