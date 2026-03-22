//! Configuration search functionality.

use std::fmt::Display;

use crate::cli_args::{FilterType, OutputMode};
use crate::resources::ResourceProvider;
use crate::search::{bindings, keyword, resource};
use crate::FullConfig;

/// Result of a configuration search operation.
///
/// The variant depends on the filter type used in the search.
pub enum SearchResult<'a> {
    /// Results from a bindings search.
    Bindings(bindings::BindingsSearchResult<'a>),
    /// Results from a keyword search.
    Keyword(keyword::KeywordSearchResult),
    /// Results from a resource search.
    Resource(resource::ResourceSearchResult),
}

impl<'a> SearchResult<'a> {
    /// Formats the search result according to the specified output mode.
    pub fn format(&self, mode: OutputMode) -> String {
        match self {
            SearchResult::Bindings(inner) => inner.to_string(),
            SearchResult::Keyword(inner) => inner.to_string(),
            SearchResult::Resource(inner) => inner.format(mode),
        }
    }
}

impl<'a> Display for SearchResult<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchResult::Bindings(inner) => inner.fmt(f),
            SearchResult::Keyword(inner) => inner.fmt(f),
            SearchResult::Resource(inner) => write!(f, "{}", inner.format(OutputMode::Summary)),
        }
    }
}

/// Executes a search on the configuration for entries matching the given pattern.
///
/// # Arguments
///
/// * `filter` - The type of search to perform
/// * `pattern` - The search pattern (case-insensitive matching)
/// * `config` - The configuration to search within
/// * `provider` - Resource provider for runtime resource values
///
/// # Returns
///
/// `Some(SearchResult)` containing matching entries. Returns `None` only
/// if the resource provider fails for a bindings search.
pub fn execute_search<'a>(
    filter: FilterType,
    pattern: &str,
    config: &'a FullConfig,
    provider: &dyn ResourceProvider,
) -> Option<SearchResult<'a>> {
    match filter {
        FilterType::Binding => {
            let trawl_resources = provider.query_resources().ok()?;
            Some(SearchResult::Bindings(bindings::search_bindings(
                pattern,
                config,
                &trawl_resources,
            )))
        }
        FilterType::Keyword => Some(SearchResult::Keyword(keyword::search_keywords(
            pattern, config,
        ))),
        FilterType::Resource => Some(SearchResult::Resource(resource::search_resources(
            pattern, config, provider,
        ))),
    }
}
