mod trawl;
mod xrdb;

pub use trawl::TrawlResourceProvider;
pub use xrdb::XrdbResourceProvider;

pub trait ResourceProvider {
    fn get_all_resources(&self) -> anyhow::Result<std::collections::HashMap<String, String>>;
}
