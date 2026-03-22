use anyhow::Result;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, LinkedList},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use crate::cli_args::Session;
use crate::search;

use super::partial::ConfigPartial;

pub type SessionMappings = [(Session, &'static Path)];

#[derive(Debug)]
pub struct FullConfig {
    _config_root: PathBuf,
    pub partials: Vec<ConfigPartial>,
}

impl FullConfig {
    pub fn new_from_session<'a>(
        session: Session,
        session_mappings: &'a SessionMappings,
    ) -> Result<Self> {
        let root_config_path = session_mappings
            .iter()
            .find_map(|&(sess, ref path)| {
                if sess == session {
                    Some(Path::new(path))
                } else {
                    None
                }
            })
            .expect("Invalid session type provided");

        let root_config = {
            let mut config_str = String::new();
            let mut root_config_file_handle = File::open(&root_config_path)?;
            root_config_file_handle.read_to_string(&mut config_str)?;
            ConfigPartial::new(&root_config_path, &config_str)
        };

        Ok(Self {
            _config_root: root_config_path.to_path_buf(),
            partials: Self::discover_config_partials(root_config)?,
        })
    }

    fn discover_config_partials(root_config: ConfigPartial) -> Result<Vec<ConfigPartial>> {
        let mut dicovered_config_partials = Vec::new();

        let mut bfs_queue = LinkedList::from([root_config]);
        let mut seen_paths = BTreeSet::new();
        while bfs_queue.len() > 0 {
            let Some(current_partial) = bfs_queue.pop_front() else {
                break;
            };

            for import_path in current_partial.get_imported_paths()? {
                if seen_paths.contains(&import_path) {
                    continue;
                }

                seen_paths.insert(import_path.clone());

                let mut import_config = String::new();
                let mut import_file_handle = File::open(&import_path)?;
                import_file_handle.read_to_string(&mut import_config)?;

                let import_partial = ConfigPartial::new(&import_path, &import_config);
                bfs_queue.push_back(import_partial);
            }

            dicovered_config_partials.push(current_partial);
        }

        Ok(dicovered_config_partials)
    }

    pub fn get_all_variables(
        &self,
        trawl_resources: &HashMap<String, String>,
    ) -> BTreeMap<String, String> {
        self.partials
            .iter()
            .flat_map(|partial| partial.config_variables(trawl_resources))
            .collect()
    }

    pub fn get_all_bindings(
        &'_ self,
        variables: &BTreeMap<String, String>,
    ) -> search::bindings::BindingsSearchResult<'_> {
        let bindings: Vec<_> = self
            .partials
            .iter()
            .flat_map(|partial| partial.config_bindings(variables))
            .collect();

        search::bindings::BindingsSearchResult::from(bindings)
    }
}
