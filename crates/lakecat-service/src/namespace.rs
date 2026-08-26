use std::num::NonZeroUsize;

use lakecat_api::ListNamespacesQuery;
use lakecat_core::{LakeCatError, LakeCatResult, Namespace};

const REST_NAMESPACE_SEPARATOR: char = '\u{001f}';
const PAGE_TOKEN_PREFIX: &str = "lakecat-v1:";
const DEFAULT_PAGE_SIZE: usize = 1_000;
const MAX_PAGE_SIZE: usize = 10_000;

pub(crate) fn parse_rest_namespace(value: &str) -> LakeCatResult<Namespace> {
    Namespace::new(
        value
            .split(REST_NAMESPACE_SEPARATOR)
            .map(str::to_string)
            .collect(),
    )
}

pub(crate) fn namespace_parent(query: &ListNamespacesQuery) -> LakeCatResult<Option<Namespace>> {
    query
        .parent
        .as_deref()
        .filter(|parent| !parent.is_empty())
        .map(parse_rest_namespace)
        .transpose()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamespacePage {
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) next_page_token: Option<String>,
}

pub(crate) fn namespace_page(
    namespaces: Vec<Namespace>,
    parent: Option<&Namespace>,
    query: &ListNamespacesQuery,
) -> LakeCatResult<NamespacePage> {
    let mut namespaces = namespaces
        .into_iter()
        .filter(|namespace| is_immediate_child(namespace, parent))
        .collect::<Vec<_>>();
    namespaces.sort();

    let Some(page_request) = PageRequest::from_query(query)? else {
        return Ok(NamespacePage {
            namespaces,
            next_page_token: None,
        });
    };
    if page_request.offset > namespaces.len() {
        return Err(LakeCatError::InvalidArgument(
            "namespace page token points beyond the available results".to_string(),
        ));
    }
    let end = page_request
        .offset
        .saturating_add(page_request.size.get())
        .min(namespaces.len());
    let next_page_token = (end < namespaces.len()).then(|| encode_page_token(end));
    Ok(NamespacePage {
        namespaces: namespaces[page_request.offset..end].to_vec(),
        next_page_token,
    })
}

fn is_immediate_child(namespace: &Namespace, parent: Option<&Namespace>) -> bool {
    match parent {
        Some(parent) => {
            namespace.parts().len() == parent.parts().len() + 1
                && namespace.parts().starts_with(parent.parts())
        }
        None => namespace.parts().len() == 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageRequest {
    offset: usize,
    size: NonZeroUsize,
}

impl PageRequest {
    fn from_query(query: &ListNamespacesQuery) -> LakeCatResult<Option<Self>> {
        if query.page_token.is_none() && query.page_size.is_none() {
            return Ok(None);
        }
        let requested_size = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
        let size = NonZeroUsize::new(requested_size.min(MAX_PAGE_SIZE)).ok_or_else(|| {
            LakeCatError::InvalidArgument("namespace page size must be at least one".to_string())
        })?;
        let offset = match query.page_token.as_deref() {
            None | Some("") => 0,
            Some(token) => decode_page_token(token)?,
        };
        Ok(Some(Self { offset, size }))
    }
}

fn encode_page_token(offset: usize) -> String {
    format!("{PAGE_TOKEN_PREFIX}{offset}")
}

fn decode_page_token(token: &str) -> LakeCatResult<usize> {
    token
        .strip_prefix(PAGE_TOKEN_PREFIX)
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(|| LakeCatError::InvalidArgument("namespace page token is invalid".to_string()))
}
