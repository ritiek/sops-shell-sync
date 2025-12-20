# sops-shell

A tool to sync secrets in a sops encrypted file based on the output of pre-defined shell commands. It supports only
the sops file formats that allow for storing comments. YAML, ENV, and INI are supported. Binary and JSON aren't
supported.

## Motivation

I use [sops-nix](https://github.com/Mic92/sops-nix) to maintain my secrets in NixOS.

Over time, I've developed use-cases for storing keys in my `secrets.yaml` using sops, that are already present in
my Bitwarden wallet.

Maintaining the same secrets again through sops has resulted in duplication of this effort. This tool helps me ease
this effort by helping treat my Bitwarden wallet as the source of truth and lets me sync specific secrets from my
wallet to `secrets.yaml`.

## Example usage

Say we have a `secrets.yaml`, which when decrypted using `sops decrypt secrets.yaml` contains the following key-value
pairs:
```yaml
postgres_user: postgresness
postgres_pass: A_STRONK_PASS
github_token: some-secret
```

We want to link the `github_token` secret to an external source. This can be done so by adding a line comment starting
with `shell:` just before the secret:
```yaml
postgres_user: postgresness
postgres_pass: A_STRONK_PASS
# shell: rbw get b8f0e379-9f78-48d5-8f06-0f62b827c663 --field "GitHub Token"
github_token: some-secret
```

Now invoke the following dry-run command (which requires setting up this project):
```bash
$ sops-shell check secrets.yaml
Processing secrets.yaml...
  Found 1 secret(s) with commands

  github_token
    Command: rbw get b8f0e379-9f78-48d5-8f06-0f62b827c663 --field "GitHub Token"
    Status: IN SYNC

  All secrets in sync

============================================================
Summary:
  Files checked: 1
  Secrets checked: 1
  Secrets out of sync: 0
```

This will execute the shell commands defined in the .yaml and assert whether the stdout of the command matches with
the secret value defined in the line just below.

Now say, this GitHub token expired, and we re-generate a new token and store it in our Bitwarden wallet. Re-running:
```bash
$ sops-shell check secrets.yaml
Processing secrets.yaml...
  Found 1 secret(s) with commands

  github_token
    Command: rbw get b8f0e379-9f78-48d5-8f06-0f62b827c663 --field "GitHub Token"
    Status: OUT OF SYNC

  Would update 1 secrets (dry run)

============================================================
Summary:
  Files checked: 1
  Secrets checked: 1
  Secrets out of sync: 1
```
would notify us that the GiHub token secret has gone out-of-sync from the output the corresponding shell command.

To re-sync all such out-of-sync secrets defined in the file (non-dry-run mode), we can execute:
```bash
$ sops-shell sync secrets.yaml
Processing secrets.yaml
  Found 1 secret(s) with commands

  github_token
    Command: rbw get b8f0e379-9f78-48d5-8f06-0f62b827c663 --field "GitHub Token"
    Status: OUT OF SYNC

  Updating 1 secrets...
    Updated github_token

  Updated secrets.yaml

============================================================
Summary:
  Files processed: 1
  Secrets checked: 1
  Secrets updated: 1
```

If we check the .yaml file now using `sops decrypt secrets.yaml`, we'll see it updated the GitHub token secret from
`some-secret` to `a-new-secret` in `secrets.yaml` as per the output of the corresponding shell command:
```yaml
postgres_user: postgresness
postgres_pass: A_STRONK_PASS
# shell: rbw get b8f0e379-9f78-48d5-8f06-0f62b827c663 --field "GitHub Token"
github_token: a-new-secret
```

## Compiling and running

If you have Nix with flakes enabled, you can use `nix run` to call the tool:
```bash
$ nix run github:ritiek/sops-shell check /path/to/secrets.yaml
```

Otherwise you can use cargo:
```bash
$ git clone https://github.com/ritiek/sops-shell
$ cd sops-shell
$ cargo run -- check /path/to/secrets.yaml
```

To see the list of supported options, pass `--help`.

## License

MIT
