pub mod bindings;
pub mod keyword;
pub mod resource;

use std::fmt::Display;

use bindings::BindingsSearchResult;
use keyword::KeywordSearchResult;
use resource::ResourceSearchResult;

use crate::cli_args::FilterType;
use crate::resources::ResourceProvider;
use crate::FullConfig;

pub enum SearchResult<'a> {
    Bindings(BindingsSearchResult<'a>),
    Keyword(KeywordSearchResult),
    Resource(ResourceSearchResult),
}

impl<'a> Display for SearchResult<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchResult::Bindings(inner) => inner.fmt(f),
            SearchResult::Keyword(inner) => inner.fmt(f),
            SearchResult::Resource(inner) => inner.fmt(f),
        }
    }
}

pub fn search_config<'a>(
    filter: FilterType,
    pattern: &str,
    config: &'a FullConfig,
    provider: &dyn ResourceProvider,
) -> Option<SearchResult<'a>> {
    match filter {
        FilterType::Bindings => {
            let trawl_resources = provider.get_all_resources().ok()?;
            Some(SearchResult::Bindings(bindings::search_binding_result(
                pattern,
                config,
                &trawl_resources,
            )))
        }
        FilterType::Keyword => Some(SearchResult::Keyword(keyword::search_keyword_result(
            pattern, config,
        ))),
        FilterType::Resource => Some(SearchResult::Resource(resource::search_resource_result(
            pattern, config, provider,
        ))),
    }
}
