use crate::*;

pub(crate) fn require_view_tombstone_expected_versions(
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let mut accepted_versions = HashMap::new();
    for (index, view) in required_array(views, "views", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        accepted_versions.insert(
            required_str(view, "stableId", "viewReceiptChainProof.views[]")?.to_string(),
            required_u64(view, "acceptedViewVersion", "viewReceiptChainProof.views[]")?,
        );
    }

    for (index, receipt) in required_array(views, "tombstoneReceipts", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let receipt = receipt.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}] must be an object"
            ))
        })?;
        let stable_id = required_str(
            receipt,
            "stableId",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let warehouse = required_str(
            receipt,
            "warehouse",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let namespace = required_array(
            receipt,
            "namespace",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let name = required_str(receipt, "name", "viewReceiptChainProof.tombstoneReceipts[]")?;
        require_view_stable_id_matches_components(
            stable_id,
            warehouse,
            namespace,
            name,
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let expected_view_version = required_u64(
            receipt,
            "expectedViewVersion",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let accepted_view_version = accepted_versions.get(stable_id).ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}] references unknown accepted view {stable_id}"
            ))
        })?;
        if expected_view_version != *accepted_view_version {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}].expectedViewVersion mismatch: expected={accepted_view_version} actual={expected_view_version}"
            )));
        }
    }

    Ok(())
}

pub(crate) fn require_bootstrap_view_receipt_hashes_match_views(
    bootstrap: &serde_json::Map<String, Value>,
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let bootstrap_hashes = required_array(
        bootstrap,
        "viewVersionReceiptHashes",
        "queryGraphBootstrapProof",
    )?;
    let view_count = required_u64(views, "viewCount", "viewReceiptChainProof")?;
    if view_count == 0 {
        if bootstrap_hashes.is_empty() {
            return Ok(());
        }
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "queryGraphBootstrapProof.viewVersionReceiptHashes must be empty when viewReceiptChainProof.viewCount is 0"
                .to_string(),
        ));
    }

    require_full_hash_array(
        bootstrap,
        "viewVersionReceiptHashes",
        "queryGraphBootstrapProof",
    )?;
    let accepted_views = required_array(views, "views", "viewReceiptChainProof")?;
    if bootstrap_hashes.len() != accepted_views.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "queryGraphBootstrapProof.viewVersionReceiptHashes length mismatch with viewReceiptChainProof.views[].acceptedReceiptHash: expected={} actual={}",
            accepted_views.len(),
            bootstrap_hashes.len()
        )));
    }

    let bootstrap_hashes = bootstrap_hashes
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let mut accepted_hashes = BTreeSet::new();
    for (index, view) in accepted_views.iter().enumerate() {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        accepted_hashes.insert(require_full_hash_str(
            view,
            "acceptedReceiptHash",
            "viewReceiptChainProof.views[]",
        )?);
    }
    if bootstrap_hashes != accepted_hashes {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "queryGraphBootstrapProof.viewVersionReceiptHashes must match viewReceiptChainProof.views[].acceptedReceiptHash exactly"
                .to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn require_view_receipt_chain_evidence(
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_view_receipt_chain_schema(views, "viewReceiptChainProof")?;
    let view_count = required_u64(views, "viewCount", "viewReceiptChainProof")?;
    if view_count == 0 {
        return Ok(());
    }

    let accepted_views = required_array(views, "views", "viewReceiptChainProof")?;
    if accepted_views.len() != view_count as usize {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.views length mismatch: expected={view_count} actual={}",
            accepted_views.len()
        )));
    }

    let mut accepted_receipt_chain_hashes = Vec::with_capacity(accepted_views.len());
    for (index, view) in accepted_views.iter().enumerate() {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        let stable_id = require_non_empty_str(view, "stableId", "viewReceiptChainProof.views[]")?;
        let warehouse = require_non_empty_str(view, "warehouse", "viewReceiptChainProof.views[]")?;
        let namespace = required_array(view, "namespace", "viewReceiptChainProof.views[]")?;
        if namespace.is_empty()
            || namespace
                .iter()
                .any(|component| !component.as_str().is_some_and(|value| !value.is_empty()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "viewReceiptChainProof.views[].namespace must contain namespace components"
                    .to_string(),
            ));
        }
        let name = require_non_empty_str(view, "name", "viewReceiptChainProof.views[]")?;
        require_view_stable_id_matches_components(
            stable_id,
            warehouse,
            namespace,
            name,
            "viewReceiptChainProof.views[]",
        )?;
        let view_version = required_u64(view, "viewVersion", "viewReceiptChainProof.views[]")?;
        let accepted_view_version =
            required_u64(view, "acceptedViewVersion", "viewReceiptChainProof.views[]")?;
        if view_version == 0 || view_version != accepted_view_version {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must prove accepted view version: viewVersion={view_version} acceptedViewVersion={accepted_view_version}"
            )));
        }
        require_full_hash_str(view, "acceptedReceiptHash", "viewReceiptChainProof.views[]")?;
        require_positive_u64(view, "graphEvents", "viewReceiptChainProof.views[]")?;
        accepted_receipt_chain_hashes.push((
            stable_id.to_string(),
            accepted_view_version,
            require_full_hash_str(
                view,
                "acceptedReceiptChainHash",
                "viewReceiptChainProof.views[]",
            )?
            .to_string(),
        ));
        require_full_hash_array(view, "replayEventHashes", "viewReceiptChainProof.views[]")?;
        require_full_hash_array(view, "openLineageHashes", "viewReceiptChainProof.views[]")?;
    }

    let mut tombstone_receipt_hashes_by_view: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (index, receipt) in required_array(views, "tombstoneReceipts", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let receipt = receipt.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}] must be an object"
            ))
        })?;
        let stable_id = required_str(
            receipt,
            "stableId",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        require_full_hash_array(
            receipt,
            "receiptHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        for hash in required_array(
            receipt,
            "receiptHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )? {
            if let Some(hash) = hash.as_str() {
                tombstone_receipt_hashes_by_view
                    .entry(stable_id.to_string())
                    .or_default()
                    .insert(hash.to_string());
            }
        }
        require_full_hash_array(
            receipt,
            "replayEventHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        require_full_hash_array(
            receipt,
            "openLineageHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
    }

    let receipt_chains = required_array(views, "receiptChains", "viewReceiptChainProof")?;
    if receipt_chains.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "viewReceiptChainProof.receiptChains must contain verified receipt-chain evidence"
                .to_string(),
        ));
    }
    let mut verified_chain_hashes_by_view: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut chain_receipt_hashes_by_view: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (index, chain) in receipt_chains.iter().enumerate() {
        let chain = chain.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{index}] must be an object"
            ))
        })?;
        require_non_empty_str(chain, "warehouse", "viewReceiptChainProof.receiptChains[]")?;
        let namespace =
            required_array(chain, "namespace", "viewReceiptChainProof.receiptChains[]")?;
        if namespace.is_empty()
            || namespace
                .iter()
                .any(|component| !component.as_str().is_some_and(|value| !value.is_empty()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "viewReceiptChainProof.receiptChains[].namespace must contain namespace components"
                    .to_string(),
            ));
        }
        let verified_chain_count = require_positive_u64(
            chain,
            "verifiedChainCount",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        let receipt_hashes = required_array(
            chain,
            "receiptHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        require_full_hash_array(
            chain,
            "receiptHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        let chain_hashes = required_array(
            chain,
            "chainHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        require_full_hash_array(
            chain,
            "chainHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        if chain_hashes.len() as u64 != verified_chain_count {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{index}].verifiedChainCount mismatch: expected={} actual={verified_chain_count}",
                chain_hashes.len()
            )));
        }
        if receipt_hashes.len() < chain_hashes.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{index}].receiptHashes must cover every verified chain hash"
            )));
        }
        require_verified_view_receipt_chain_structures(
            chain,
            index,
            &mut verified_chain_hashes_by_view,
            &mut chain_receipt_hashes_by_view,
        )?;
        require_full_hash_array(
            chain,
            "replayEventHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        require_full_hash_array(
            chain,
            "openLineageHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
    }
    for (stable_id, accepted_view_version, accepted_chain_hash) in accepted_receipt_chain_hashes {
        if !verified_chain_hashes_by_view
            .get(&stable_id)
            .is_some_and(|hashes| hashes.contains(&accepted_chain_hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[] accepted view {stable_id} version {accepted_view_version} acceptedReceiptChainHash {accepted_chain_hash} is not covered by the same view's receiptChains[].chains[].chainHash"
            )));
        }
    }
    for (stable_id, tombstone_receipt_hashes) in tombstone_receipt_hashes_by_view {
        if !chain_receipt_hashes_by_view
            .get(&stable_id)
            .is_some_and(|chain_hashes| tombstone_receipt_hashes.is_subset(chain_hashes))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[].receiptHashes for {stable_id} must be covered by the same view's receiptChains[].chains[].receipts[].receiptHash"
            )));
        }
    }

    Ok(())
}

pub(crate) fn require_view_receipt_chain_schema(
    views: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_only_fields(views, VIEW_RECEIPT_CHAIN_PROOF_FIELDS, label)?;
    for (index, view) in required_array(views, "views", label)?.iter().enumerate() {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.views[{index}] must be an object"
            ))
        })?;
        let view_label = format!("{label}.views[]");
        require_only_fields(view, VIEW_RECEIPT_CHAIN_VIEW_FIELDS, &view_label)?;
    }
    for (index, receipt) in required_array(views, "tombstoneReceipts", label)?
        .iter()
        .enumerate()
    {
        let receipt = receipt.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.tombstoneReceipts[{index}] must be an object"
            ))
        })?;
        let receipt_label = format!("{label}.tombstoneReceipts[]");
        require_only_fields(receipt, VIEW_RECEIPT_CHAIN_TOMBSTONE_FIELDS, &receipt_label)?;
    }
    for (group_index, chain_group) in required_array(views, "receiptChains", label)?
        .iter()
        .enumerate()
    {
        let chain_group = chain_group.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.receiptChains[{group_index}] must be an object"
            ))
        })?;
        let chain_group_label = format!("{label}.receiptChains[]");
        require_only_fields(
            chain_group,
            VIEW_RECEIPT_CHAIN_GROUP_FIELDS,
            &chain_group_label,
        )?;
        for (chain_index, chain) in required_array(chain_group, "chains", &chain_group_label)?
            .iter()
            .enumerate()
        {
            let chain = chain.as_object().ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "{label}.receiptChains[{group_index}].chains[{chain_index}] must be an object"
                ))
            })?;
            let chain_label = format!("{label}.receiptChains[].chains[]");
            require_only_fields(chain, VIEW_RECEIPT_CHAIN_CHAIN_FIELDS, &chain_label)?;
            for (receipt_index, receipt) in required_array(chain, "receipts", &chain_label)?
                .iter()
                .enumerate()
            {
                let receipt = receipt.as_object().ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(format!(
                        "{label}.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}] must be an object"
                    ))
                })?;
                let receipt_label = format!("{label}.receiptChains[].chains[].receipts[]");
                require_only_fields(receipt, VIEW_RECEIPT_CHAIN_RECEIPT_FIELDS, &receipt_label)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn require_view_stable_id_matches_components(
    stable_id: &str,
    warehouse: &str,
    namespace: &[Value],
    name: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let namespace_path = namespace_components_path(namespace, label)?;
    let expected = format!("lakecat:view:{warehouse}:{namespace_path}:{name}");
    if stable_id != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.stableId must match warehouse/namespace/name: expected={expected} actual={stable_id}"
        )));
    }
    Ok(())
}

pub(crate) fn namespace_components_path(
    namespace: &[Value],
    label: &str,
) -> lakecat_core::LakeCatResult<String> {
    let mut components = Vec::with_capacity(namespace.len());
    for (index, component) in namespace.iter().enumerate() {
        let Some(component) = component.as_str().filter(|component| !component.is_empty()) else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.namespace[{index}] must be a non-empty string"
            )));
        };
        components.push(component);
    }
    if components.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.namespace must contain namespace components"
        )));
    }
    Ok(components.join("."))
}

pub(crate) fn require_verified_view_receipt_chain_structures(
    chain_group: &serde_json::Map<String, Value>,
    group_index: usize,
    verified_chain_hashes_by_view: &mut BTreeMap<String, BTreeSet<String>>,
    chain_receipt_hashes_by_view: &mut BTreeMap<String, BTreeSet<String>>,
) -> lakecat_core::LakeCatResult<()> {
    let verified_chain_count = require_positive_u64(
        chain_group,
        "verifiedChainCount",
        "viewReceiptChainProof.receiptChains[]",
    )?;
    let chain_hashes = required_array(
        chain_group,
        "chainHashes",
        "viewReceiptChainProof.receiptChains[]",
    )?
    .iter()
    .filter_map(Value::as_str)
    .collect::<BTreeSet<_>>();
    let declared_chain_hash_count = required_array(
        chain_group,
        "chainHashes",
        "viewReceiptChainProof.receiptChains[]",
    )?
    .len();
    if declared_chain_hash_count != chain_hashes.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chainHashes must not contain duplicate hashes"
        )));
    }
    let receipt_hashes = required_array(
        chain_group,
        "receiptHashes",
        "viewReceiptChainProof.receiptChains[]",
    )?
    .iter()
    .filter_map(Value::as_str)
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    let declared_receipt_hash_count = required_array(
        chain_group,
        "receiptHashes",
        "viewReceiptChainProof.receiptChains[]",
    )?
    .len();
    if declared_receipt_hash_count != receipt_hashes.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].receiptHashes must not contain duplicate hashes"
        )));
    }
    let chains = required_array(
        chain_group,
        "chains",
        "viewReceiptChainProof.receiptChains[]",
    )?;
    if chains.len() as u64 != verified_chain_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains length mismatch: expected={verified_chain_count} actual={}",
            chains.len()
        )));
    }
    let group_warehouse = required_str(
        chain_group,
        "warehouse",
        "viewReceiptChainProof.receiptChains[]",
    )?;
    let group_namespace = required_array(
        chain_group,
        "namespace",
        "viewReceiptChainProof.receiptChains[]",
    )?;

    let mut structural_receipt_hashes = BTreeSet::new();
    let mut structural_chain_hashes = BTreeSet::new();
    for (chain_index, chain) in chains.iter().enumerate() {
        let chain = chain.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}] must be an object"
            ))
        })?;
        require_verified_view_receipt_chain_structure(
            chain,
            group_index,
            chain_index,
            &chain_hashes,
            group_warehouse,
            group_namespace,
            verified_chain_hashes_by_view,
            chain_receipt_hashes_by_view,
            &mut structural_chain_hashes,
            &mut structural_receipt_hashes,
        )?;
    }
    if structural_receipt_hashes != receipt_hashes {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].receiptHashes must match receiptChains[].chains[].receipts[].receiptHash exactly"
        )));
    }
    Ok(())
}

pub(crate) fn require_verified_view_receipt_chain_structure(
    chain: &serde_json::Map<String, Value>,
    group_index: usize,
    chain_index: usize,
    chain_hashes: &BTreeSet<&str>,
    group_warehouse: &str,
    group_namespace: &[Value],
    verified_chain_hashes_by_view: &mut BTreeMap<String, BTreeSet<String>>,
    chain_receipt_hashes_by_view: &mut BTreeMap<String, BTreeSet<String>>,
    structural_chain_hashes: &mut BTreeSet<String>,
    structural_receipt_hashes: &mut BTreeSet<String>,
) -> lakecat_core::LakeCatResult<()> {
    let label = "viewReceiptChainProof.receiptChains[].chains[]";
    let stable_id = require_non_empty_str(chain, "stableId", label)?;
    let warehouse = require_non_empty_str(chain, "warehouse", label)?;
    if warehouse != group_warehouse {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].warehouse must match receipt-chain group warehouse"
        )));
    }
    let namespace = required_array(chain, "namespace", label)?;
    if namespace.is_empty()
        || namespace
            .iter()
            .any(|component| !component.as_str().is_some_and(|value| !value.is_empty()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].namespace must contain namespace components"
        )));
    }
    if namespace != group_namespace {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].namespace must match receipt-chain group namespace"
        )));
    }
    let name = require_non_empty_str(chain, "name", label)?;
    require_view_stable_id_matches_components(stable_id, warehouse, namespace, name, label)?;
    let chain_hash = require_full_hash_str(chain, "chainHash", label)?;
    if !chain_hashes.contains(chain_hash) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].chainHash is not covered by chainHashes"
        )));
    }
    if !structural_chain_hashes.insert(chain_hash.to_string()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[] must not contain duplicate chainHash values"
        )));
    }
    verified_chain_hashes_by_view
        .entry(stable_id.to_string())
        .or_default()
        .insert(chain_hash.to_string());
    if !required_bool(chain, "chainVerified", label)? {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].chainVerified must be true"
        )));
    }
    let latest_view_version = require_positive_u64(chain, "latestViewVersion", label)?;
    let latest_operation = required_str(chain, "latestOperation", label)?;
    if !matches!(latest_operation, "upsert" | "drop") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].latestOperation is unsupported"
        )));
    }
    let tombstoned = required_bool(chain, "tombstoned", label)?;
    if tombstoned != (latest_operation == "drop") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].tombstoned must match latestOperation"
        )));
    }
    let receipt_count = require_positive_u64(chain, "receiptCount", label)?;
    let receipts = required_array(chain, "receipts", label)?;
    if receipts.len() as u64 != receipt_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receiptCount mismatch: expected={} actual={receipt_count}",
            receipts.len()
        )));
    }
    require_verified_view_receipts(
        receipts,
        group_index,
        chain_index,
        latest_view_version,
        latest_operation,
        stable_id,
        warehouse,
        namespace,
        name,
        chain_receipt_hashes_by_view,
        structural_receipt_hashes,
    )?;
    let computed_chain_hash = view_receipt_chain_hash_from_compact_structure(
        stable_id,
        warehouse,
        namespace,
        name,
        latest_view_version,
        latest_operation,
        tombstoned,
        receipts,
    )?;
    if chain_hash != computed_chain_hash {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].chainHash must match the structural receipt-chain digest"
        )));
    }
    Ok(())
}

pub(crate) fn require_verified_view_receipts(
    receipts: &[Value],
    group_index: usize,
    chain_index: usize,
    latest_view_version: u64,
    latest_operation: &str,
    expected_stable_id: &str,
    expected_warehouse: &str,
    expected_namespace: &[Value],
    expected_name: &str,
    chain_receipt_hashes_by_view: &mut BTreeMap<String, BTreeSet<String>>,
    structural_receipt_hashes: &mut BTreeSet<String>,
) -> lakecat_core::LakeCatResult<()> {
    let mut previous_view_version: Option<u64> = None;
    let mut previous_receipt_hash: Option<String> = None;
    let mut latest_receipt_operation = None;
    let mut latest_receipt_version = None;

    for (receipt_index, receipt) in receipts.iter().enumerate() {
        let receipt = receipt.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}] must be an object"
            ))
        })?;
        let label = "viewReceiptChainProof.receiptChains[].chains[].receipts[]";
        if require_non_empty_str(receipt, "stableId", label)? != expected_stable_id {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}].stableId must match chain stableId"
            )));
        }
        if require_non_empty_str(receipt, "warehouse", label)? != expected_warehouse {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}].warehouse must match chain warehouse"
            )));
        }
        let namespace = required_array(receipt, "namespace", label)?;
        if namespace.is_empty()
            || namespace
                .iter()
                .any(|component| !component.as_str().is_some_and(|value| !value.is_empty()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}].namespace must contain namespace components"
            )));
        }
        if namespace != expected_namespace {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}].namespace must match chain namespace"
            )));
        }
        if require_non_empty_str(receipt, "name", label)? != expected_name {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}].name must match chain name"
            )));
        }
        let view_version = require_positive_u64(receipt, "viewVersion", label)?;
        let operation = required_str(receipt, "operation", label)?;
        let receipt_hash = require_full_hash_str(receipt, "receiptHash", label)?;

        if receipt_index == 0 {
            if operation != "upsert"
                || view_version != 1
                || !required_value(receipt, "previousViewVersion", label)?.is_null()
                || !required_value(receipt, "previousReceiptHash", label)?.is_null()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[0] must be a version 1 upsert without previous links"
                )));
            }
        } else {
            if required_u64(receipt, "previousViewVersion", label)?
                != previous_view_version.unwrap()
                || required_str(receipt, "previousReceiptHash", label)?
                    != previous_receipt_hash.as_deref().unwrap()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}] previous links must match the prior receipt"
                )));
            }
            match operation {
                "upsert" if view_version == previous_view_version.unwrap().saturating_add(1) => {}
                "drop" if view_version == previous_view_version.unwrap() => {}
                _ => {
                    return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                        "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}] transition is invalid"
                    )));
                }
            }
        }
        let computed_receipt_hash = view_receipt_hash_from_compact_structure(receipt, label)?;
        if receipt_hash != computed_receipt_hash {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}].receipts[{receipt_index}].receiptHash must match the structural view receipt digest"
            )));
        }
        chain_receipt_hashes_by_view
            .entry(expected_stable_id.to_string())
            .or_default()
            .insert(receipt_hash.to_string());
        structural_receipt_hashes.insert(receipt_hash.to_string());
        latest_receipt_operation = Some(operation.to_string());
        latest_receipt_version = Some(view_version);
        previous_view_version = Some(view_version);
        previous_receipt_hash = Some(receipt_hash.to_string());
    }

    if latest_receipt_version != Some(latest_view_version)
        || latest_receipt_operation.as_deref() != Some(latest_operation)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.receiptChains[{group_index}].chains[{chain_index}] latest receipt does not match chain head"
        )));
    }
    Ok(())
}

pub(crate) fn view_receipt_hash_from_compact_structure(
    receipt: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<String> {
    let mut value = serde_json::Map::new();
    value.insert(
        "stable-id".to_string(),
        Value::String(require_non_empty_str(receipt, "stableId", label)?.to_string()),
    );
    value.insert(
        "warehouse".to_string(),
        Value::String(require_non_empty_str(receipt, "warehouse", label)?.to_string()),
    );
    value.insert(
        "namespace".to_string(),
        Value::Array(required_array(receipt, "namespace", label)?.to_vec()),
    );
    value.insert(
        "name".to_string(),
        Value::String(require_non_empty_str(receipt, "name", label)?.to_string()),
    );
    value.insert(
        "view-version".to_string(),
        Value::Number(required_u64(receipt, "viewVersion", label)?.into()),
    );
    let previous_view_version = required_value(receipt, "previousViewVersion", label)?;
    if !previous_view_version.is_null() {
        value.insert(
            "previous-view-version".to_string(),
            Value::Number(required_u64(receipt, "previousViewVersion", label)?.into()),
        );
    }
    let previous_receipt_hash = required_value(receipt, "previousReceiptHash", label)?;
    if !previous_receipt_hash.is_null() {
        value.insert(
            "previous-receipt-hash".to_string(),
            Value::String(
                require_full_hash_str(receipt, "previousReceiptHash", label)?.to_string(),
            ),
        );
    }
    value.insert(
        "operation".to_string(),
        Value::String(required_str(receipt, "operation", label)?.to_string()),
    );
    value.insert(
        "view-hash".to_string(),
        Value::String(require_full_hash_str(receipt, "viewHash", label)?.to_string()),
    );
    value.insert(
        "principal".to_string(),
        json!({
            "subject": require_non_empty_str(receipt, "principalSubject", label)?,
            "kind": require_non_empty_str(receipt, "principalKind", label)?,
        }),
    );
    value.insert(
        "recorded-at".to_string(),
        Value::String(normalized_utc_recorded_at(receipt, label)?),
    );
    content_hash_json(&Value::Object(value))
}

pub(crate) fn normalized_utc_recorded_at(
    receipt: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<String> {
    let recorded_at = require_non_empty_str(receipt, "recordedAt", label)?;
    let parsed = DateTime::parse_from_rfc3339(recorded_at).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.recordedAt must be an RFC3339 timestamp: {err}"
        ))
    })?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::AutoSi, true))
}

pub(crate) fn view_receipt_chain_hash_from_compact_structure(
    stable_id: &str,
    warehouse: &str,
    namespace: &[Value],
    name: &str,
    latest_view_version: u64,
    latest_operation: &str,
    tombstoned: bool,
    receipts: &[Value],
) -> lakecat_core::LakeCatResult<String> {
    let receipt_hashes = receipts
        .iter()
        .map(|receipt| {
            let receipt = receipt.as_object().ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(
                    "viewReceiptChainProof receipt-chain digest requires object receipts"
                        .to_string(),
                )
            })?;
            Ok(required_str(
                receipt,
                "receiptHash",
                "viewReceiptChainProof.receiptChains[].chains[].receipts[]",
            )?
            .to_string())
        })
        .collect::<lakecat_core::LakeCatResult<Vec<_>>>()?;
    content_hash_json(&json!({
        "stable-id": stable_id,
        "warehouse": warehouse,
        "namespace": namespace,
        "name": name,
        "latest-view-version": latest_view_version,
        "latest-operation": latest_operation,
        "tombstoned": tombstoned,
        "receipt-hashes": receipt_hashes,
    }))
}
