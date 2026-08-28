use std::collections::BTreeMap;

use lakecat_core::{LakeCatError, Namespace, WarehouseName};

use crate::{CatalogStore, MemoryCatalogStore, NamespaceProperties, NamespacePropertyUpdate};

fn properties(entries: &[(&str, &str)]) -> NamespaceProperties {
    NamespaceProperties::new(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    )
    .unwrap()
}

#[test]
fn namespace_property_update_is_pure_deterministic_and_validated() {
    let before = properties(&[("owner", "finance"), ("remove", "before")]);
    let update = NamespacePropertyUpdate::new(
        vec!["remove".to_string(), "missing".to_string()],
        BTreeMap::from([("state".to_string(), "after".to_string())]),
    )
    .unwrap();
    let (after, result) = before.apply(&update);

    assert_eq!(
        before.as_map().get("remove").map(String::as_str),
        Some("before")
    );
    assert_eq!(
        after.as_map().get("owner").map(String::as_str),
        Some("finance")
    );
    assert_eq!(
        after.as_map().get("state").map(String::as_str),
        Some("after")
    );
    assert!(!after.as_map().contains_key("remove"));
    assert_eq!(result.updated, vec!["state"]);
    assert_eq!(result.removed, vec!["remove"]);
    assert_eq!(result.missing, vec!["missing"]);

    let overlap = NamespacePropertyUpdate::new(
        vec!["owner".to_string()],
        BTreeMap::from([("owner".to_string(), "other".to_string())]),
    )
    .unwrap_err();
    assert!(matches!(overlap, LakeCatError::UnprocessableEntity(_)));
    let duplicate = NamespacePropertyUpdate::new(
        vec!["owner".to_string(), "owner".to_string()],
        BTreeMap::new(),
    )
    .unwrap_err();
    assert!(matches!(duplicate, LakeCatError::InvalidArgument(_)));
}

async fn assert_namespace_store(store: &dyn CatalogStore) {
    let warehouse = WarehouseName::new("local").unwrap();
    let parent = Namespace::new(vec!["accounting".to_string()]).unwrap();
    let child = Namespace::new(vec!["accounting".to_string(), "tax".to_string()]).unwrap();
    store
        .create_namespace_with_properties(
            &warehouse,
            parent.clone(),
            properties(&[("owner", "finance"), ("remove", "before")]),
        )
        .await
        .unwrap();
    store
        .create_namespace(&warehouse, child.clone())
        .await
        .unwrap();

    let loaded = store
        .load_namespace_properties(&warehouse, &parent)
        .await
        .unwrap();
    assert_eq!(
        loaded.as_map().get("owner").map(String::as_str),
        Some("finance")
    );
    let result = store
        .update_namespace_properties(
            &warehouse,
            &parent,
            NamespacePropertyUpdate::new(
                vec!["remove".to_string(), "absent".to_string()],
                BTreeMap::from([("state".to_string(), "after".to_string())]),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(result.updated, vec!["state"]);
    assert_eq!(result.removed, vec!["remove"]);
    assert_eq!(result.missing, vec!["absent"]);

    let conflict = store.drop_namespace(&warehouse, &parent).await.unwrap_err();
    assert!(matches!(conflict, LakeCatError::Conflict(_)));
    store.drop_namespace(&warehouse, &child).await.unwrap();
    store.drop_namespace(&warehouse, &parent).await.unwrap();
    assert!(matches!(
        store.load_namespace_properties(&warehouse, &parent).await,
        Err(LakeCatError::NotFound {
            object: "namespace",
            ..
        })
    ));
}

#[tokio::test]
async fn memory_namespace_properties_are_atomic_and_hierarchy_safe() {
    let store = MemoryCatalogStore::new();
    assert_namespace_store(store.as_ref()).await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_namespace_properties_are_atomic_and_hierarchy_safe() {
    let store = crate::turso_store::TursoCatalogStore::in_memory()
        .await
        .unwrap();
    assert_namespace_store(store.as_ref()).await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_namespaces_without_property_rows_migrate_lazily() {
    let store = crate::turso_store::TursoCatalogStore::in_memory()
        .await
        .unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = Namespace::new(vec!["legacy".to_string()]).unwrap();
    let conn = store.database().connect().unwrap();
    conn.execute(
        "insert into namespaces (warehouse, namespace_path, namespace_json) values (?1, ?2, ?3)",
        (warehouse.as_str(), namespace.storage_key(), r#"["legacy"]"#),
    )
    .await
    .unwrap();

    let loaded = store
        .load_namespace_properties(&warehouse, &namespace)
        .await
        .unwrap();
    assert!(loaded.is_empty());

    let result = store
        .update_namespace_properties(
            &warehouse,
            &namespace,
            NamespacePropertyUpdate::new(
                Vec::new(),
                BTreeMap::from([("owner".to_string(), "platform".to_string())]),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(result.updated, vec!["owner"]);
    assert_eq!(
        store
            .load_namespace_properties(&warehouse, &namespace)
            .await
            .unwrap()
            .as_map()
            .get("owner")
            .map(String::as_str),
        Some("platform")
    );
}
