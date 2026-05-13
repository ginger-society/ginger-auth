

// ── helper: write a file and set Unix permissions ─────────────────────────────

use std::{fs, io::Write, process::{Command, Stdio}};
#[cfg(unix)]
use std::{fs::File, path::PathBuf};
// ── helper: write a file and set Unix permissions ─────────────────────────────

#[cfg(unix)]
pub fn write_ssh_file(path: &PathBuf, content: &str, mode: u32) {
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
pub fn write_ssh_file(path: &PathBuf, content: &str, _mode: u32) {
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

pub fn update_npmrc(home_dir: &PathBuf, token: &str, slug: &str) {
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

pub fn update_pypirc(home_dir: &PathBuf, token: &str, _slug: &str) {
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

pub fn upsert_line(lines: &mut Vec<String>, key_prefix: &str, new_line: &str) {
    if let Some(pos) = lines.iter().position(|l| l.starts_with(key_prefix)) {
        lines[pos] = new_line.to_string();
    } else {
        lines.push(new_line.to_string());
    }
}


// ── docker login ──────────────────────────────────────────────────────────────
// Runs: echo <token> | docker login registry.gingersociety.org -u __token__ --password-stdin
// Non-fatal: if docker isn't installed or the registry is unreachable we just warn.
pub fn docker_login(token: &str) {
    let (tool, args): (&str, &[&str]) = if is_command_available("docker") {
        ("docker", &["login", "docker.gingersociety.org", "-u", "__token__", "--password-stdin"])
    } else if is_command_available("buildah") {
        ("buildah", &["login", "docker.gingersociety.org", "-u", "__token__", "--password-stdin"])
    } else {
        println!("Warning: neither docker nor buildah found; writing ~/.docker/config.json directly");
        write_docker_config(token);
        return;
    };

    let mut child = match Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("Warning: could not launch {}: {}", tool, e);
            return;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(token.as_bytes()) {
            println!("Warning: could not write to {} stdin: {}", tool, e);
            return;
        }
    }

    match child.wait_with_output() {
        Ok(output) if output.status.success() => {
            println!("{} login succeeded (docker.gingersociety.org)", tool);
        }
        Ok(output) => {
            println!(
                "Warning: {} login failed: {}",
                tool,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(e) => println!("Warning: {} login error: {}", tool, e),
    }
}

fn write_docker_config(token: &str) {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let auth = STANDARD.encode(format!("__token__:{}", token));
    let config = format!(
        r#"{{
  "auths": {{
    "docker.gingersociety.org": {{
      "auth": "{}"
    }}
  }}
}}"#,
        auth
    );

    let docker_dir = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".docker");

    if let Err(e) = fs::create_dir_all(&docker_dir) {
        println!("Warning: could not create ~/.docker: {}", e);
        return;
    }

    let config_path = docker_dir.join("config.json");

    // If config.json already exists, merge the new auth in rather than overwriting
    let final_config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(existing) => merge_docker_config(&existing, "docker.gingersociety.org", &auth),
            Err(_) => config,
        }
    } else {
        config
    };

    match File::create(&config_path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(final_config.as_bytes()) {
                println!("Warning: could not write ~/.docker/config.json: {}", e);
            } else {
                println!("~/.docker/config.json written (docker.gingersociety.org)");
            }
        }
        Err(e) => println!("Warning: could not create ~/.docker/config.json: {}", e),
    }
}

fn merge_docker_config(existing: &str, registry: &str, auth: &str) -> String {
    // Best-effort JSON merge — if parsing fails, append the registry entry
    // A proper serde_json merge avoids clobbering other registries the user
    // may already be logged into (e.g. ghcr.io, docker.io)
    match serde_json::from_str::<serde_json::Value>(existing) {
        Ok(mut json) => {
            json["auths"][registry]["auth"] = serde_json::Value::String(auth.to_string());
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| existing.to_string())
        }
        Err(_) => {
            println!("Warning: existing ~/.docker/config.json is not valid JSON; overwriting");
            format!(
                r#"{{
  "auths": {{
    "{}": {{
      "auth": "{}"
    }}
  }}
}}"#,
                registry, auth
            )
        }
    }
}

fn is_command_available(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}