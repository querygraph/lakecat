use crate::*;

pub(crate) enum Command {
    BootstrapExport {
        catalog: String,
        output: PathBuf,
        principal: Option<String>,
    },
    Config {
        catalog: String,
        principal: Option<String>,
    },
    LineageDrain {
        catalog: String,
        principal: Option<String>,
    },
    QglakeVerifyReplay {
        bundle: PathBuf,
        drain: PathBuf,
        principal: Option<String>,
        json: bool,
    },
    QglakeVerifyHandoff {
        summary: PathBuf,
        json: bool,
    },
    PolicyList {
        catalog: String,
        warehouse: String,
        principal: Option<String>,
    },
    PolicyUpsert {
        catalog: String,
        warehouse: String,
        policy: String,
        namespace: Option<Vec<String>>,
        table: Option<String>,
        enforced: bool,
        odrl: Value,
        principal: Option<String>,
    },
    StorageProfileList {
        catalog: String,
        warehouse: String,
        principal: Option<String>,
    },
    StorageProfileUpsert {
        catalog: String,
        warehouse: String,
        profile: String,
        location_prefix: String,
        provider: String,
        issuance_mode: String,
        secret_ref: Option<String>,
        public_config: BTreeMap<String, String>,
        principal: Option<String>,
    },
    #[cfg(feature = "qglake-fixture")]
    QglakeFixture {
        catalog: String,
        warehouse: String,
        namespace: Vec<String>,
        table: String,
        location: String,
        metadata_location: String,
        output: PathBuf,
        drain_output: Option<PathBuf>,
        principal: Option<String>,
    },
}

impl Command {
    pub(crate) fn parse(
        args: impl IntoIterator<Item = String>,
    ) -> lakecat_core::LakeCatResult<Self> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Err(usage_error());
        };
        match command.as_str() {
            "bootstrap-export" => parse_bootstrap_export(args),
            "config" => parse_config(args),
            "lineage-drain" => parse_lineage_drain(args),
            "qglake-verify-replay" => parse_qglake_verify_replay(args),
            "qglake-verify-handoff" => parse_qglake_verify_handoff(args),
            "policy-list" => parse_policy_list(args),
            "policy-upsert" => parse_policy_upsert(args),
            "storage-profile-list" => parse_storage_profile_list(args),
            "storage-profile-upsert" => parse_storage_profile_upsert(args),
            #[cfg(feature = "qglake-fixture")]
            "qglake-fixture" => parse_qglake_fixture(args),
            #[cfg(not(feature = "qglake-fixture"))]
            "qglake-fixture" => Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake-fixture requires the lakecat-cli qglake-fixture feature".to_string(),
            )),
            _ => Err(usage_error()),
        }
    }
}

pub(crate) fn parse_bootstrap_export(
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

pub(crate) fn parse_config(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
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

pub(crate) fn parse_lineage_drain(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
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
    Ok(Command::LineageDrain { catalog, principal })
}

pub(crate) fn parse_qglake_verify_replay(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut bundle = None;
    let mut drain = None;
    let mut principal = None;
    let mut json = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bundle" => bundle = Some(PathBuf::from(next_arg(&mut args, "--bundle")?)),
            "--drain" => drain = Some(PathBuf::from(next_arg(&mut args, "--drain")?)),
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            "--json" => json = true,
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::QglakeVerifyReplay {
        bundle: bundle.ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "missing required --bundle for qglake-verify-replay".to_string(),
            )
        })?,
        drain: drain.ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "missing required --drain for qglake-verify-replay".to_string(),
            )
        })?,
        principal,
        json,
    })
}

pub(crate) fn parse_qglake_verify_handoff(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut summary = None;
    let mut json = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--summary" => summary = Some(PathBuf::from(next_arg(&mut args, "--summary")?)),
            "--json" => json = true,
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::QglakeVerifyHandoff {
        summary: summary.ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "missing required --summary for qglake-verify-handoff".to_string(),
            )
        })?,
        json,
    })
}

pub(crate) fn parse_policy_list(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::PolicyList {
        catalog: common.catalog,
        warehouse: common.warehouse,
        principal: common.principal,
    })
}

pub(crate) fn parse_policy_upsert(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut policy = None;
    let mut namespace = None;
    let mut table = None;
    let mut enforced = true;
    let mut odrl = json!({});
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            "--policy" => policy = Some(next_arg(&mut args, "--policy")?),
            "--namespace" => {
                namespace = Some(parse_namespace(&next_arg(&mut args, "--namespace")?))
            }
            "--table" => table = Some(next_arg(&mut args, "--table")?),
            "--enforced" => enforced = parse_bool(&next_arg(&mut args, "--enforced")?)?,
            "--odrl-file" => {
                odrl = read_json_file(&PathBuf::from(next_arg(&mut args, "--odrl-file")?))?
            }
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::PolicyUpsert {
        catalog: common.catalog,
        warehouse: common.warehouse,
        policy: required(policy, "--policy")?,
        namespace,
        table,
        enforced,
        odrl,
        principal: common.principal,
    })
}

pub(crate) fn parse_storage_profile_list(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::StorageProfileList {
        catalog: common.catalog,
        warehouse: common.warehouse,
        principal: common.principal,
    })
}

pub(crate) fn parse_storage_profile_upsert(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut profile = None;
    let mut location_prefix = None;
    let mut provider = None;
    let mut issuance_mode = None;
    let mut secret_ref = None;
    let mut public_config = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            "--profile" => profile = Some(next_arg(&mut args, "--profile")?),
            "--location-prefix" => {
                location_prefix = Some(next_arg(&mut args, "--location-prefix")?)
            }
            "--provider" => provider = Some(next_arg(&mut args, "--provider")?),
            "--issuance-mode" => issuance_mode = Some(next_arg(&mut args, "--issuance-mode")?),
            "--secret-ref" => secret_ref = Some(next_arg(&mut args, "--secret-ref")?),
            "--public-config" => {
                let (key, value) = parse_key_value(&next_arg(&mut args, "--public-config")?)?;
                public_config.insert(key, value);
            }
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::StorageProfileUpsert {
        catalog: common.catalog,
        warehouse: common.warehouse,
        profile: required(profile, "--profile")?,
        location_prefix: required(location_prefix, "--location-prefix")?,
        provider: required(provider, "--provider")?,
        issuance_mode: required(issuance_mode, "--issuance-mode")?,
        secret_ref,
        public_config,
        principal: common.principal,
    })
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn parse_qglake_fixture(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut namespace = vec!["default".to_string()];
    let mut table = "events".to_string();
    let mut location = "file:///tmp/lakecat-qglake/events".to_string();
    let mut metadata_location = "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string();
    let mut output = PathBuf::from("target/qglake/lakecat-bootstrap.json");
    let mut drain_output = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            "--namespace" => namespace = parse_namespace(&next_arg(&mut args, "--namespace")?),
            "--table" => table = next_arg(&mut args, "--table")?,
            "--location" => location = next_arg(&mut args, "--location")?,
            "--metadata-location" => {
                metadata_location = next_arg(&mut args, "--metadata-location")?
            }
            "--output" => output = PathBuf::from(next_arg(&mut args, "--output")?),
            "--drain-output" => {
                drain_output = Some(PathBuf::from(next_arg(&mut args, "--drain-output")?))
            }
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::QglakeFixture {
        catalog: common.catalog,
        warehouse: common.warehouse,
        namespace,
        table,
        location,
        metadata_location,
        output,
        drain_output,
        principal: common.principal,
    })
}

pub(crate) struct CommonArgs {
    catalog: String,
    warehouse: String,
    principal: Option<String>,
}

impl Default for CommonArgs {
    fn default() -> Self {
        Self {
            catalog: "http://127.0.0.1:8181".to_string(),
            warehouse: "local".to_string(),
            principal: None,
        }
    }
}

pub(crate) fn required(value: Option<String>, flag: &str) -> lakecat_core::LakeCatResult<String> {
    value.ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("missing required {flag}"))
    })
}

pub(crate) fn parse_namespace(value: &str) -> Vec<String> {
    value
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn parse_key_value(value: &str) -> lakecat_core::LakeCatResult<(String, String)> {
    let Some((key, value)) = value.split_once('=') else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "--public-config values must use key=value".to_string(),
        ));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "--public-config key cannot be empty".to_string(),
        ));
    }
    Ok((key.to_string(), value.to_string()))
}

pub(crate) fn parse_bool(value: &str) -> lakecat_core::LakeCatResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "invalid boolean value: {other}"
        ))),
    }
}

pub(crate) fn read_json_file(path: &PathBuf) -> lakecat_core::LakeCatResult<Value> {
    let bytes = fs::read(path).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to read JSON file {}: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to parse JSON file {}: {err}",
            path.display()
        ))
    })
}

pub(crate) fn read_typed_json_file<T: DeserializeOwned>(
    path: &PathBuf,
    label: &str,
) -> lakecat_core::LakeCatResult<T> {
    serde_json::from_value(read_json_file(path)?).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} JSON file {} did not match expected shape: {err}",
            path.display()
        ))
    })
}

pub(crate) fn required_value<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a Value> {
    value.get(field).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} is missing required field {field}"
        ))
    })
}

pub(crate) fn required_object<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a serde_json::Map<String, Value>> {
    required_value(value, field, label)?
        .as_object()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.{field} must be an object"
            ))
        })
}

pub(crate) fn require_only_fields(
    value: &serde_json::Map<String, Value>,
    allowed_fields: &[&str],
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    for field in value.keys() {
        if !allowed_fields.iter().any(|allowed| allowed == field) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} contains unexpected field {field}"
            )));
        }
    }
    Ok(())
}

/// Tolerant-by-policy field check for artifacts produced by the *QueryGraph
/// importer* (its captured verify/import output and import-plan). The importer may
/// enrich these with additional descriptive or per-view evidence fields over time
/// (catalog labels, table-node counts, `verified-view-*` maps, and future ones),
/// so LakeCat does NOT gate on their exact shape — round-trip integrity is enforced
/// separately by strict matching of the security-critical hashes, counts, and ids.
/// This requires the known core fields are present but tolerates any extras.
///
/// Use this only for importer-produced artifacts. LakeCat's OWN replay proof claims
/// stay strict via [`require_only_fields`] so forged/unverified claims are rejected.
pub(crate) fn require_present_fields(
    value: &serde_json::Map<String, Value>,
    required_fields: &[&str],
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    for field in required_fields {
        if !value.contains_key(*field) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} is missing required field {field}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn required_array<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a Vec<Value>> {
    value.get(field).and_then(Value::as_array).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("{label}.{field} must be an array"))
    })
}

pub(crate) fn required_string_array(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<Vec<String>> {
    required_array(value, field, label)?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_str().map(ToString::to_string).ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "{label}.{field}[{index}] must be a string"
                ))
            })
        })
        .collect()
}

pub(crate) fn require_non_empty_unique_strings(
    values: &[String],
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} must contain non-empty strings"
        )));
    }
    let mut seen = BTreeSet::new();
    if values.iter().any(|value| !seen.insert(value.as_str())) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} must be duplicate-free"
        )));
    }
    Ok(())
}

pub(crate) fn required_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    required_value(value, field, label)?
        .as_str()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!("{label}.{field} must be a string"))
        })
}

pub(crate) fn required_bool(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<bool> {
    value.get(field).and_then(Value::as_bool).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("{label}.{field} must be a boolean"))
    })
}

pub(crate) fn require_absent_or_null_field(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if value.get(field).is_some_and(|value| !value.is_null()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be absent or null"
        )));
    }
    Ok(())
}

pub(crate) fn required_u64(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<u64> {
    value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be a non-negative integer"
        ))
    })
}

pub(crate) fn require_positive_u64(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<u64> {
    let number = required_u64(value, field, label)?;
    if number == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be positive"
        )));
    }
    Ok(number)
}

pub(crate) fn require_unique_string_array_count(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected_count: u64,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let values = required_string_array(value, field, label)?;
    if values.len() as u64 != expected_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} count mismatch: expected={expected_count} actual={}",
            values.len()
        )));
    }
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must contain non-empty strings"
        )));
    }
    if values
        .iter()
        .any(|value| !is_qglake_compact_management_id(value))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} contains syntactically invalid compact management ID evidence"
        )));
    }
    let mut seen = BTreeSet::new();
    if values.iter().any(|value| !seen.insert(value)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be duplicate-free"
        )));
    }
    Ok(())
}

pub(crate) fn require_non_empty_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    let string = required_str(value, field, label)?;
    if string.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must not be empty"
        )));
    }
    Ok(string)
}

pub(crate) fn require_non_blank_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    let string = require_non_empty_str(value, field, label)?;
    require_non_blank_input(string, format!("{label}.{field}").as_str())
}

pub(crate) fn require_non_blank_input<'a>(
    string: &'a str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    if string.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} must not be blank"
        )));
    }
    Ok(string)
}

pub(crate) fn require_hash_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    let string = require_non_empty_str(value, field, label)?;
    if !is_sha256_hash(string) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be a sha256 hash"
        )));
    }
    Ok(string)
}

pub(crate) fn require_full_hash_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    let string = require_non_empty_str(value, field, label)?;
    if !is_full_sha256_hash(string) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be a full SHA-256 hash"
        )));
    }
    Ok(string)
}

pub(crate) fn is_sha256_hash(string: &str) -> bool {
    string.starts_with("sha256:")
}

pub(crate) fn is_full_sha256_hash(string: &str) -> bool {
    let Some(digest) = string.strip_prefix("sha256:") else {
        return false;
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn require_optional_hash_value(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<bool> {
    match required_value(value, field, label)? {
        Value::Null => Ok(false),
        Value::String(string) if is_full_sha256_hash(string) => Ok(true),
        _ => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be null or a full SHA-256 hash"
        ))),
    }
}

pub(crate) fn require_full_hash_array(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let array = required_array(value, field, label)?;
    if array.is_empty()
        || array
            .iter()
            .any(|item| !item.as_str().is_some_and(is_full_sha256_hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must contain full SHA-256 hashes"
        )));
    }
    let mut seen = BTreeSet::new();
    if array
        .iter()
        .filter_map(Value::as_str)
        .any(|hash| !seen.insert(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be duplicate-free"
        )));
    }
    Ok(())
}

pub(crate) fn require_string_eq(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_string_match(value, field, expected, label)
}

pub(crate) fn require_string_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let actual = required_str(value, field, label)?;
    if actual != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} mismatch: expected={expected} actual={actual}"
        )));
    }
    Ok(())
}

pub(crate) fn require_u64_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: u64,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let actual = required_u64(value, field, label)?;
    if actual != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} mismatch: expected={expected} actual={actual}"
        )));
    }
    Ok(())
}

pub(crate) fn require_value_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: &Value,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let actual = value.get(field).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} is missing required field {field}"
        ))
    })?;
    if actual != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} mismatch"
        )));
    }
    Ok(())
}

pub(crate) fn require_optional_null_value_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: Option<&Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    match expected {
        Some(expected) if !expected.is_null() => require_value_match(value, field, expected, label),
        _ => require_absent_or_null_field(value, field, label),
    }
}

pub(crate) fn next_arg(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> lakecat_core::LakeCatResult<String> {
    args.next().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("missing value for {flag}"))
    })
}

pub(crate) fn usage_error() -> lakecat_core::LakeCatError {
    let commands = [
        "config",
        "bootstrap-export",
        "lineage-drain",
        "qglake-verify-replay",
        "qglake-verify-handoff",
        "storage-profile-list",
        "storage-profile-upsert",
        "policy-list",
        "policy-upsert",
        "qglake-fixture",
    ];
    lakecat_core::LakeCatError::InvalidArgument(format!(
        "usage: lakecat-cli <{}> [options]",
        commands.join("|")
    ))
}
