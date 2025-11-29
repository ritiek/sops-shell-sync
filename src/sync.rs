use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::parser::parse_commands;
use crate::sops::{sops_decrypt, sops_set};

fn print_file_error(operation: &str, error: &anyhow::Error) {
    println!("  Error: Failed to {}: {}", operation, error);
}

fn print_command_error(error: &anyhow::Error) {
    println!("    Error: Command failed");
    for msg in error.chain() {
        println!("    {}", msg);
    }
}

pub fn execute_command(command: &str) -> Result<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .envs(std::env::vars())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("Failed to execute command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Command failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn parse_decrypted_value(decrypted_content: &str, key: &str) -> Option<String> {
    decrypted_content.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed.strip_prefix(key)
                .and_then(|rest| rest.trim().strip_prefix(':'))
                .map(|value_part| value_part.trim().trim_matches('"').to_string())
        })
}

pub fn process_file(filepath: &Path, dry_run: bool) -> Result<(usize, usize)> {
    println!("\nProcessing {}...", filepath.display());

    let decrypted = match sops_decrypt(filepath) {
        Ok(content) => content,
        Err(e) => {
            print_file_error("decrypt", &e);
            return Ok((0, 0));
        }
    };

    let mappings = match parse_commands(&decrypted) {
        Ok(m) => m,
        Err(e) => {
            print_file_error("parse commands", &e);
            return Ok((0, 0));
        }
    };

    if mappings.is_empty() {
        println!("  No secrets with 'shell:' commands found");
        return Ok((0, 0));
    }

    println!("  Found {} secrets with commands\n", mappings.len());

    let mut updates = Vec::new();

    for mapping in &mappings {
        println!("  {}", mapping.key);
        println!("    Command: {}", mapping.command);

        match execute_command(&mapping.command) {
            Ok(value) => {
                let current_value = parse_decrypted_value(&decrypted, &mapping.key);

                if Some(&value) != current_value.as_ref() {
                    updates.push((mapping.key.clone(), value.clone()));
                    println!("    Status: OUT OF SYNC");
                } else {
                    println!("    Status: IN SYNC");
                }
            }
            Err(e) => {
                print_command_error(&e);
            }
        }
    }

    if !updates.is_empty() {
        if dry_run {
            println!("\n  Would update {} secrets (dry run)", updates.len());
        } else {
            println!("\n  Updating {} secrets...", updates.len());

            for (key, value) in &updates {
                match sops_set(filepath, key, value) {
                    Ok(()) => {
                        println!("    Updated {}", key);
                    }
                    Err(e) => {
                        println!("    Error updating {}: {}", key, e);
                    }
                }
            }

            println!("\n  Updated {}", filepath.display());
        }
    } else {
        println!("\n  All secrets in sync");
    }

    Ok((mappings.len(), updates.len()))
}

pub fn process_files(files: &[impl AsRef<Path>], dry_run: bool) -> Result<()> {
    let mut total_secrets = 0;
    let mut total_updates = 0;

    for file in files {
        let (secrets, updates) = process_file(file.as_ref(), dry_run)?;
        total_secrets += secrets;
        total_updates += updates;
    }

    print_summary(files.len(), total_secrets, total_updates, dry_run);

    Ok(())
}

pub fn sync_files(files: &[impl AsRef<Path>]) -> Result<()> {
    process_files(files, false)
}

pub fn check_files(files: &[impl AsRef<Path>]) -> Result<()> {
    process_files(files, true)
}

fn print_summary(files_count: usize, total_secrets: usize, total_updates: usize, dry_run: bool) {
    println!("\n{}", "=".repeat(60));
    println!("Summary:");
    if dry_run {
        println!("  Files checked: {}", files_count);
        println!("  Secrets checked: {}", total_secrets);
        println!("  Secrets out of sync: {}", total_updates);

        if total_updates > 0 {
            println!("\nRun 'sops-shell sync <files>' to update");
        }
    } else {
        println!("  Files processed: {}", files_count);
        println!("  Secrets checked: {}", total_secrets);
        println!("  Secrets updated: {}", total_updates);
    }
}
