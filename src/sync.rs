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
            !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with(';')
        })
        .find_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(key) {
                if let Some(value_part) = rest.trim().strip_prefix('=') {
                    return Some(value_part.trim().trim_matches('"').to_string());
                }
                if let Some(value_part) = rest.trim().strip_prefix(':') {
                    return Some(value_part.trim().trim_matches('"').to_string());
                }
            }
            None
        })
}

fn has_comment_lines(filepath: &Path) -> Result<bool> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(filepath)?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(100) {
        let line = line?;
        if line.trim_start().starts_with('#') || line.trim_start().starts_with(';') {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn process_file(filepath: &Path, dry_run: bool) -> Result<(usize, usize)> {
    println!("\nProcessing {}...", filepath.display());

    if !has_comment_lines(filepath)? {
        println!("  No comment lines found, skipping decryption");
        return Ok((0, 0));
    }

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
        println!("  No secret(s) with 'shell:' commands found");
        return Ok((0, 0));
    }

    println!("  Found {} secret(s) with commands\n", mappings.len());

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(content: &str) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(content.as_bytes()).expect("Failed to write to temp file");
        temp_file
    }

    mod has_comment_lines {
        use super::*;

        #[test]
        fn test_yaml_with_comments() {
            let yaml_content = r#"#ENC[AES256_GCM,data:XZKYdNp090c0OssVxy3nsbVjJyQEdxDs2+w6nXs21Pq8JeUGB3wHjuuaD1uz/OTIO1nF7jM0ZSb0ED5/oTxMRSkbXa3PtRDScFulxzgs9oqt,iv:WsFYuBa3wdEMSX8hklDVJCbJkwbuk2mUSrGh9819pEk=,tag:QRNJOJnTulAcu45x1rc70A==,type:comment]
key1: ENC[AES256_GCM,data:z3lcZi7luB/Ni5FFIB8h98qLZDofk9g8KsQRIhVRwexmZSNihuTp58nzEueBQt8D,iv:MuIReuZYNIX0zi9hOBmpTK8tFhO+MtA27tvwimGusf4=,tag:CnwlixiANK6HGwb2GweIVw==,type:str]
value2: ENC[AES256_GCM,data:hLAuHEcvnSXSuy/8UFnLrScX7o121EPEwNHKZQOqnIdyY1EM6w6Ej0zKAREHuBsoblZRoD6a8OHTd1CrWE/3oNmKt0Y5SbY693Y9HLTC716B7/gBzRhtqpxZL3vICG7MXfrI/f4dKtx0Kroro+69PU8/6hxi7WmMVk6hL2BnipUSlINho6ev/AZuKOY+x6HAPTXE34BwK0SkmgkFgviLU1BQRNwGc+rVvmItXZLrOhL7iOSfmlOZm7qTcxOUp8oNLX+RECxLzhLr/4fj4jlB8JaNWanCvBcCim12nS5Shzsl/ylnvczcYttFAQeqjU2it8Q1lF4QcHJDg1kiNYdVBG1AiD3x1JEYR34M2tAPAcnX1uoVoZAofMjiHOaz5mWV8MahLJDt9XU+J4I/y2X7qduyXPLlXf+tnIu5D5p+WMMC,iv:fq3nwR7vMx9E270pGPTyrxENaQQAF8EInvZX3Y+JMIQ=,tag:wNcFV/e8/nXDpAqLaRGIHw==,type:str]"#;

            let temp_file = create_test_file(yaml_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(result, "YAML with comments should return true");
        }

        #[test]
        fn test_yaml_without_comments() {
            let yaml_content = r#"key1: ENC[AES256_GCM,data:z3lcZi7luB/Ni5FFIB8h98qLZDofk9g8KsQRIhVRwexmZSNihuTp58nzEueBQt8D,iv:MuIReuZYNIX0zi9hOBmpTK8tFhO+MtA27tvwimGusf4=,tag:CnwlixiANK6HGwb2GweIVw==,type:str]
value2: ENC[AES256_GCM,data:hLAuHEcvnSXSuy/8UFnLrScX7o121EPEwNHKZQOqnIdyY1EM6w6Ej0zKAREHuBsoblZRoD6a8OHTd1CrWE/3oNmKt0Y5SbY693Y9HLTC716B7/gBzRhtqpxZL3vICG7MXfrI/f4dKtx0Kroro+69PU8/6hxi7WmMVk6hL2BnipUSlINho6ev/AZuKOY+x6HAPTXE34BwK0SkmgkFgviLU1BQRNwGc+rVvmItXZLrOhL7iOSfmlOZm7qTcxOUp8oNLX+RECxLzhLr/4fj4jlB8JaNWanCvBcCim12nS5Shzsl/ylnvczcYttFAQeqjU2it8Q1lF4QcHJDg1kiNYdVBG1AiD3x1JEYR34M2tAPAcnX1uoVoZAofMjiHOaz5mWV8MahLJDt9XU+J4I/y2X7qduyXPLlXf+tnIu5D5p+WMMC,iv:fq3nwR7vMx9E270pGPTyrxENaQQAF8EInvZX3Y+JMIQ=,tag:wNcFV/e8/nXDpAqLaRGIHw==,type:str]
secret3: ENC[AES256_GCM,data:vbMDk21AfoTzfLgybWt52JytPaf103wqmQjxhJMG/WPul2eGtmcjlez/+BNb9xgYlNvxRt00ET2lpDx5WlBQylTWuUm/tlfXBM4Iu1vXsjRAsF8X/Q==,iv:BW8bJULqt+H///8aA9z43Vs7E+mAE4YrcOIjFnvg6Fk=,tag:Srt8W4GD7LICubVzI4DmXA==,type:str]"#;

            let temp_file = create_test_file(yaml_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(!result, "YAML without comments should return false");
        }

        #[test]
        fn test_env_with_comments() {
            let env_content = r#"VAR1=ENC[AES256_GCM,data:plVGuA==,iv:G8F5FbNqTrcMnekeojVDiLnHm/P3zLK2J4hH0hAORdM=,tag:KRlxAFtLwqsnToYyH0gv5Q==,type:str]
VAR2=ENC[AES256_GCM,data:Z1YXLA==,iv:RK8JpogKYXjtRZSvTKMLbHLIqmPcdg+XUyFIw4ScRN8=,tag:G7wq3M+eyqiRbuv+1CbgDg==,type:str]
#ENC[AES256_GCM,data:EKnMX0aQPqY1y8FDE98ePqM3t8A/Jc6dX3IqVwhwuRkMaIOmO9Ak4iQtso17iCzVHcQ=,iv:8kKEKYUVeRul6GOWxc7HZQsSYAHJwqCXMajC2N1xgNI=,tag:Pb/hHvJQSLe4pQc9isqjqA==,type:comment]
VAR3=ENC[AES256_GCM,data:z9jujEfYa8Y=,iv:E4CoMsU1OU1+QZErLIQquoiDI23vuDbXmBDtamL1kNk=,tag:ahQR7j8+RHJUO4mymWeMJw==,type:str]"#;

            let temp_file = create_test_file(env_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(result, "ENV file with comments should return true");
        }

        #[test]
        fn test_env_without_comments() {
            let env_content = r#"VAR1=ENC[AES256_GCM,data:plVGuA==,iv:G8F5FbNqTrcMnekeojVDiLnHm/P3zLK2J4hH0hAORdM=,tag:KRlxAFtLwqsnToYyH0gv5Q==,type:str]
VAR2=ENC[AES256_GCM,data:Z1YXLA==,iv:RK8JpogKYXjtRZSvTKMLbHLIqmPcdg+XUyFIw4ScRN8=,tag:G7wq3M+eyqiRbuv+1CbgDg==,type:str]
VAR3=ENC[AES256_GCM,data:z9jujEfYa8Y=,iv:E4CoMsU1OU1+QZErLIQquoiDI23vuDbXmBDtamL1kNk=,tag:ahQR7j8+RHJUO4mymWeMJw==,type:str]"#;

            let temp_file = create_test_file(env_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(!result, "ENV file without comments should return false");
        }

        #[test]
        fn test_ini_with_comments() {
            let ini_content = r#"[config]
option1 = ENC[AES256_GCM,data:mJ5DKLvrKtiJ,iv:21wg1JWyntnndyHnyox1dwH4iA58CBUphqPmDvKz6b0=,tag:G+MxG7dK0KccQPpm6B8U3w==,type:str]
option2    = ENC[AES256_GCM,data:0kk=,iv:+K69YIFcTuL3T3SzlPGcZkdSLSnoixo2CtM/lUwpZ7o=,tag:uPV3xSjHzJUjh0Urv6KN/Q==,type:str]
; ENC[AES256_GCM,data:zYAvfyP84D8YP2Svs4+0W9M2eF7KhmS+17JIw7lIjeSIaTsdLCuzpgCbRL6ZfbCVPQci,iv:FAr+Za/MsmRqg1DTzPLcuOCH53a8n5UUQsqGE6Kv1p4=,tag:OOcIjJv+Tt+X+2QI2bdOOw==,type:comment]
option3    = ENC[AES256_GCM,data:1GWHjsNTwQTA51jp+BOyHX+FhlF7CPfx6kyU0k658iG/k440NzIR4VK9qcFXrIrmQjhIwJaG5YxK5TiuK/qN7uuY2omz5gB0oHA=,iv:hDHT4OIUP6gZ27qGdHCkZoFc+hMkOFZ7EK9L/OBRLus=,tag:IaAF3T5RhVeDmg3eXU7Ymw==,type:str]"#;

            let temp_file = create_test_file(ini_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(result, "INI file with comments should return true");
        }

        #[test]
        fn test_ini_without_comments() {
            let ini_content = r#"[config]
option1 = ENC[AES256_GCM,data:mJ5DKLvrKtiJ,iv:21wg1JWyntnndyHnyox1dwH4iA58CBUphqPmDvKz6b0=,tag:G+MxG7dK0KccQPpm6B8U3w==,type:str]
option2    = ENC[AES256_GCM,data:0kk=,iv:+K69YIFcTuL3T3SzlPGcZkdSLSnoixo2CtM/lUwpZ7o=,tag:uPV3xSjHzJUjh0Urv6KN/Q==,type:str]
option3     = ENC[AES256_GCM,data:dGVzdA==,iv:abc123==,tag:test123==,type:str]"#;

            let temp_file = create_test_file(ini_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(!result, "INI file without comments should return false");
        }

        #[test]
        fn test_whitespace_before_comments() {
            let content = r#"key: ENC[AES256_GCM,data:test,iv:test,tag:test,type:str]
    #ENC[AES256_GCM,data:indented,iv:test,tag:test,type:comment]
value: ENC[AES256_GCM,data:test,iv:test,tag:test,type:str]"#;

            let temp_file = create_test_file(content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(result, "Should detect comments with whitespace prefix");
        }

        #[test]
        fn test_tab_before_comments() {
            let content = r#"key: ENC[AES256_GCM,data:test,iv:test,tag:test,type:str]
	#ENC[AES256_GCM,data:tabbed,iv:test,tag:test,type:comment]
value: ENC[AES256_GCM,data:test,iv:test,tag:test,type:str]"#;

            let temp_file = create_test_file(content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(result, "Should detect comments with tab prefix");
        }

        #[test]
        fn test_empty_file() {
            let content = r#""#;

            let temp_file = create_test_file(content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(!result, "Empty file should return false");
        }

        #[test]
        fn test_only_empty_lines() {
            let content = r#"


"#;

            let temp_file = create_test_file(content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(!result, "File with only empty lines should return false");
        }

        #[test]
        fn test_shell_comments_specifically() {
            let yaml_content = r#"#ENC[AES256_GCM,data:XZKYdNp090c0OssVxy3nsbVjJyQEdxDs2+w6nXs21Pq8JeUGB3wHjuuaD1uz/OTIO1nF7jM0ZSb0ED5/oTxMRSkbXa3PtRDScFulxzgs9oqt,iv:WsFYuBa3wdEMSX8hklDVJCbJkwbuk2mUSrGh9819pEk=,tag:QRNJOJnTulAcu45x1rc70A==,type:comment]
key: ENC[AES256_GCM,data:test,iv:test,tag:test,type:str]"#;

            let temp_file = create_test_file(yaml_content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(result, "Should detect SOPS encrypted comment lines");
        }

        #[test]
        fn test_large_file_without_comments() {
            let mut content = String::new();
            for i in 0..150 {
                content.push_str(&format!("line{}: ENC[AES256_GCM,data:test{}{},iv:test123,tag:test456,type:str]\n", i, i, i));
            }

            let temp_file = create_test_file(&content);
            let result = has_comment_lines(temp_file.path()).expect("Should not fail");
            assert!(!result, "Large file without comments should return false");
        }
    }
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
