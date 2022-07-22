use std::collections::BTreeMap;

use crate::{cli, options::ProfileOpt, utils::client::StudioClientConfig};

use super::{SchemaSource, SubgraphConfig};
use apollo_federation_types::build::SubgraphDefinition;
use buildstructor::buildstructor;
use reqwest::Url;
use saucer::{anyhow, Result, Utf8Path};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MultiSubgraphConfig {
    supergraph: ExtendSupergraphConfig,

    // Store config in a BTreeMap, as HashMap is non-deterministic.
    subgraphs: BTreeMap<String, SubgraphConfig>,
}

#[buildstructor]
impl MultiSubgraphConfig {
    pub(crate) fn new() -> Self {
        Self {
            supergraph: ExtendSupergraphConfig { graph_id: None },
            subgraphs: BTreeMap::new(),
        }
    }

    #[builder(entry = "supergraph", exit = "extend")]
    pub(crate) fn extend_supergraph(&mut self, graph_id: String) -> Result<()> {
        if let Some(graph_id) = &self.supergraph.graph_id {
            Err(anyhow!(
                "supergraph with graph ID {} already exists",
                graph_id
            ))
        } else {
            self.supergraph = ExtendSupergraphConfig {
                graph_id: Some(graph_id),
            };
            Ok(())
        }
    }

    #[builder(entry = "subgraph", exit = "add")]
    pub(crate) fn add_subgraph(&mut self, name: String, config: SubgraphConfig) -> Result<()> {
        if self.subgraphs.get(&name).is_some() {
            Err(anyhow!(
                "could not extend subgraph config because {} already exists",
                &name
            ))
        } else {
            self.subgraphs.insert(name, config);
            Ok(())
        }
    }

    pub(crate) fn edit_subgraph(&mut self, name: &str, remote_endpoint: &str) -> Result<()> {
        if let Some(config) = self.subgraphs.get_mut(name) {
            config.edit_remote_endpoint(remote_endpoint.parse()?);
            Ok(())
        } else {
            Err(anyhow!(
                "subgraph with name '{}' is not defined in .apollo/config.yaml",
                name
            ))
        }
    }

    pub(crate) fn get_supergraph(&self) -> ExtendSupergraphConfig {
        self.supergraph.clone()
    }

    pub(crate) fn get_subgraph(&self, name: &str) -> Option<SubgraphConfig> {
        self.subgraphs.get(name).map(|s| s.clone())
    }

    pub(crate) fn try_get_only_subgraph(&mut self) -> Result<(String, SubgraphConfig)> {
        if self.subgraphs.len() == 1 {
            let (name, config) = self.subgraphs.iter().next().unwrap();
            Ok((name.to_string(), config.clone()))
        } else {
            Err(anyhow!(
                ".apollo/config.yaml contains more than one subgraph"
            ))
        }
    }

    pub(crate) fn get_all_subgraphs(
        &self,
        dev: bool,
        client_config: &StudioClientConfig,
        profile_opt: &ProfileOpt,
    ) -> Result<Vec<SubgraphDefinition>> {
        let mut defs = Vec::new();
        for (name, config) in &self.subgraphs {
            let url = config.url(dev)?;
            let sdl = config.sdl(client_config, profile_opt)?;
            defs.push(SubgraphDefinition::new(name, url, sdl));
        }
        Ok(defs)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtendSupergraphConfig {
    graph_id: Option<String>,
}

impl ExtendSupergraphConfig {
    pub fn graph_id(&self) -> Option<String> {
        self.graph_id.clone()
    }
}
