use std::{fs, path::PathBuf};

use lakecat_api::CatalogConfigResponse;
use lakecat_querygraph::QueryGraphBootstrap;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("lakecat-cli: {err}");
        std::process::exit(1);
    }
}

async fn run() -> lakecat_core::LakeCatResult<()> {
    let command = Command::parse(std::env::args().skip(1))?;
    match command {
        Command::BootstrapExport {
            catalog,
            output,
            principal,
        } => bootstrap_export(catalog, output, principal).await,
        Command::Config { catalog, principal } => config(catalog, principal).await,
    }
}

async fn bootstrap_export(
    catalog: String,
    output: PathBuf,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let endpoint = format!("{}/querygraph/v1/bootstrap", catalog.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request bootstrap bundle: {err}"))
    })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to read bootstrap response: {err}"))
    })?;
    if !status.is_success() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "bootstrap export failed with HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    let bundle: QueryGraphBootstrap = serde_json::from_slice(&body).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "LakeCat bootstrap response is not a QueryGraph bundle: {err}"
        ))
    })?;
    let verification = bundle.verify_manifest()?;
    let pretty = serde_json::to_vec_pretty(&bundle).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to encode bootstrap bundle: {err}"))
    })?;
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to create output directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    fs::write(&output, pretty).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write bootstrap bundle {}: {err}",
            output.display()
        ))
    })?;
    println!(
        "wrote {} table(s) for warehouse {} to {}",
        verification.table_count,
        verification.warehouse,
        output.display()
    );
    println!("bundle {}", verification.bundle_hash);
    Ok(())
}

async fn config(catalog: String, principal: Option<String>) -> lakecat_core::LakeCatResult<()> {
    let endpoint = format!("{}/catalog/v1/config", catalog.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request catalog config: {err}"))
    })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to read config response: {err}"))
    })?;
    if !status.is_success() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "catalog config failed with HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    let config: CatalogConfigResponse = serde_json::from_slice(&body).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "LakeCat config response is not an Iceberg REST config payload: {err}"
        ))
    })?;
    let pretty = serde_json::to_string_pretty(&config).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to encode config response: {err}"))
    })?;
    println!("{pretty}");
    Ok(())
}

enum Command {
    BootstrapExport {
        catalog: String,
        output: PathBuf,
        principal: Option<String>,
    },
    Config {
        catalog: String,
        principal: Option<String>,
    },
}

impl Command {
    fn parse(args: impl IntoIterator<Item = String>) -> lakecat_core::LakeCatResult<Self> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Err(usage_error());
        };
        match command.as_str() {
            "bootstrap-export" => parse_bootstrap_export(args),
            "config" => parse_config(args),
            _ => Err(usage_error()),
        }
    }
}

fn parse_bootstrap_export(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut catalog = "http://127.0.0.1:8181".to_string();
    let mut output = None;
    let mut principal = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => catalog = next_arg(&mut args, "--catalog")?,
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    let output = output.ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "missing required --output for bootstrap-export".to_string(),
        )
    })?;
    Ok(Command::BootstrapExport {
        catalog,
        output,
        principal,
    })
}

fn parse_config(args: impl Iterator<Item = String>) -> lakecat_core::LakeCatResult<Command> {
    let mut catalog = "http://127.0.0.1:8181".to_string();
    let mut principal = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => catalog = next_arg(&mut args, "--catalog")?,
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::Config { catalog, principal })
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> lakecat_core::LakeCatResult<String> {
    args.next().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("missing value for {flag}"))
    })
}

fn usage_error() -> lakecat_core::LakeCatError {
    lakecat_core::LakeCatError::InvalidArgument(
        "usage: lakecat-cli <config|bootstrap-export> [--catalog URL] [--principal NAME] [--output PATH]"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_command_defaults() {
        let command = Command::parse(["config".to_string()]).unwrap();
        match command {
            Command::Config { catalog, principal } => {
                assert_eq!(catalog, "http://127.0.0.1:8181");
                assert_eq!(principal, None);
            }
            Command::BootstrapExport { .. } => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_bootstrap_export_command() {
        let command = Command::parse([
            "bootstrap-export".to_string(),
            "--catalog".to_string(),
            "http://localhost:9000".to_string(),
            "--output".to_string(),
            "bundle.json".to_string(),
            "--principal".to_string(),
            "alice".to_string(),
        ])
        .unwrap();
        match command {
            Command::BootstrapExport {
                catalog,
                output,
                principal,
            } => {
                assert_eq!(catalog, "http://localhost:9000");
                assert_eq!(output, PathBuf::from("bundle.json"));
                assert_eq!(principal.as_deref(), Some("alice"));
            }
            Command::Config { .. } => panic!("expected bootstrap-export command"),
        }
    }
}
