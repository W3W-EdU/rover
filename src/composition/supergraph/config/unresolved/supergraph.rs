//! Provides tooling to resolve subgraphs, fully or lazily
use std::collections::BTreeMap;

use apollo_federation_types::config::{FederationVersion, SubgraphConfig};
use buildstructor::buildstructor;
use camino::Utf8PathBuf;
use derive_getters::Getters;

use super::UnresolvedSubgraph;

/// Object that represents a [`SupergraphConfig`] that requires resolution
#[derive(Getters)]
pub struct UnresolvedSupergraphConfig {
    origin_path: Option<Utf8PathBuf>,
    subgraphs: BTreeMap<String, UnresolvedSubgraph>,
    federation_version: Option<FederationVersion>,
}

#[buildstructor]
impl UnresolvedSupergraphConfig {
    /// Hook for [`buildstructor::buildstructor`]'s builder pattern
    #[builder]
    pub fn new(
        origin_path: Option<Utf8PathBuf>,
        subgraphs: BTreeMap<String, SubgraphConfig>,
        federation_version: Option<FederationVersion>,
    ) -> UnresolvedSupergraphConfig {
        let subgraphs = BTreeMap::from_iter(
            subgraphs
                .into_iter()
                .map(|(name, config)| (name.to_string(), UnresolvedSubgraph::new(name, config))),
        );
        UnresolvedSupergraphConfig {
            origin_path,
            subgraphs,
            federation_version,
        }
    }
}

#[cfg(test)]
mod tests {

    use std::{
        collections::{BTreeMap, HashSet},
        str::FromStr,
    };

    use anyhow::Result;
    use apollo_federation_types::config::{FederationVersion, SchemaSource};
    use assert_fs::TempDir;
    use camino::Utf8PathBuf;
    use mockall::predicate;
    use rstest::{fixture, rstest};
    use speculoos::prelude::*;

    use crate::{
        composition::supergraph::config::{
            full::{FullyResolvedSubgraph, FullyResolvedSupergraphConfig},
            lazy::{LazilyResolvedSubgraph, LazilyResolvedSupergraphConfig},
            resolver::ResolveSupergraphConfigError,
            scenario::*,
            unresolved::UnresolvedSupergraphConfig,
        },
        utils::effect::{
            fetch_remote_subgraph::{MockFetchRemoteSubgraph, RemoteSubgraph},
            introspect::MockIntrospectSubgraph,
        },
    };

    #[fixture]
    fn supergraph_config_root_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[rstest]
    // All subgraphs are fed one, no version has been specified, so we default to LatestFedOne
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::One),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        None,
        FederationVersion::LatestFedTwo
    )]
    // All subgraphs are fed two, no version has been specified, so we infer LatestFedTwo
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::Two),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        None,
        FederationVersion::LatestFedTwo
    )]
    // One subgraph is fed two, no version has been specified, so we infer LatestFedTwo
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::Two),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        None,
        FederationVersion::LatestFedTwo
    )]
    // All subgraphs are fed one, fed one is specified, so we default to LatestFedOne
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::One),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        Some(FederationVersion::LatestFedOne),
        FederationVersion::LatestFedOne
    )]
    // All subgraphs are fed two, fed two is specified, so we default to LatestFedTwo
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::Two),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        Some(FederationVersion::LatestFedTwo),
        FederationVersion::LatestFedTwo
    )]
    // One subgraph is fed two, fed two is specified, so we infer LatestFedTwo
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::Two),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        Some(FederationVersion::LatestFedTwo),
        FederationVersion::LatestFedTwo
    )]
    // All subgraphs are fed one, fed two is specified, so we default to LatestFedTwo
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::One),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        Some(FederationVersion::LatestFedTwo),
        FederationVersion::LatestFedTwo
    )]
    #[tokio::test]
    async fn test_fully_resolve_subgraphs_success(
        supergraph_config_root_dir: TempDir,
        #[case] sdl_subgraph_scenario: SdlSubgraphScenario,
        #[case] remote_subgraph_scenario: RemoteSubgraphScenario,
        #[case] introspect_subgraph_scenario: IntrospectSubgraphScenario,
        #[case] file_subgraph_scenario: FileSubgraphScenario,
        // this version is expected to have been resolved from local and remote supergraph resolution steps already
        #[case] target_federation_version: Option<FederationVersion>,
        #[case] expected_federation_version: FederationVersion,
    ) -> Result<()> {
        file_subgraph_scenario.write_schema_file(supergraph_config_root_dir.path())?;
        let mut unresolved_subgraphs = BTreeMap::new();
        let sdl_subgraph_name = "sdl_subgraph".to_string();
        unresolved_subgraphs.insert(
            sdl_subgraph_name.clone(),
            sdl_subgraph_scenario.unresolved_subgraph,
        );
        let remote_subgraph_name = "remote_subgraph".to_string();
        unresolved_subgraphs.insert(
            remote_subgraph_name.clone(),
            remote_subgraph_scenario.unresolved_subgraph,
        );
        let introspect_subgraph_name = "introspect_subgraph".to_string();
        unresolved_subgraphs.insert(
            introspect_subgraph_name.clone(),
            introspect_subgraph_scenario.unresolved_subgraph,
        );
        let file_subgraph_name = "file_subgraph".to_string();
        unresolved_subgraphs.insert(
            file_subgraph_name.clone(),
            file_subgraph_scenario.unresolved_subgraph,
        );

        let unresolved_supergraph_config = UnresolvedSupergraphConfig {
            origin_path: None,
            subgraphs: unresolved_subgraphs,
            federation_version: target_federation_version,
        };

        let RemoteSubgraphScenario {
            sdl: ref remote_subgraph_sdl,
            graph_ref: remote_subgraph_graph_ref,
            subgraph_name: remote_subgraph_subgraph_name,
            routing_url: remote_subgraph_routing_url,
            ..
        } = remote_subgraph_scenario;

        let mut mock_fetch_remote_subgraph = MockFetchRemoteSubgraph::new();
        mock_fetch_remote_subgraph
            .expect_fetch_remote_subgraph()
            .times(1)
            .with(
                predicate::eq(remote_subgraph_graph_ref.clone()),
                predicate::eq(remote_subgraph_subgraph_name.to_string()),
            )
            .returning({
                let remote_subgraph_sdl = remote_subgraph_sdl.to_string();
                let remote_subgraph_routing_url = remote_subgraph_routing_url.to_string();
                move |_, name| {
                    Ok(RemoteSubgraph::builder()
                        .name(name.to_string())
                        .routing_url(remote_subgraph_routing_url.to_string())
                        .schema(remote_subgraph_sdl.to_string())
                        .build())
                }
            });

        let IntrospectSubgraphScenario {
            sdl: ref introspect_subgraph_sdl,
            routing_url: introspect_subgraph_routing_url,
            introspection_headers: introspect_subgraph_introspection_headers,
            ..
        } = introspect_subgraph_scenario;

        let mut mock_introspect_subgraph = MockIntrospectSubgraph::new();
        mock_introspect_subgraph
            .expect_introspect_subgraph()
            .times(1)
            .with(
                predicate::eq(url::Url::from_str(&introspect_subgraph_routing_url)?),
                predicate::eq(introspect_subgraph_introspection_headers),
            )
            .returning({
                let introspect_subgraph_sdl = introspect_subgraph_sdl.to_string();
                move |_, _| Ok(introspect_subgraph_sdl.to_string())
            });

        let result = FullyResolvedSupergraphConfig::resolve(
            &mock_introspect_subgraph,
            &mock_fetch_remote_subgraph,
            Some(
                &Utf8PathBuf::from_path_buf(supergraph_config_root_dir.path().to_path_buf())
                    .unwrap(),
            ),
            unresolved_supergraph_config,
        )
        .await;

        mock_fetch_remote_subgraph.checkpoint();
        mock_introspect_subgraph.checkpoint();

        let resolved_supergraph_config = assert_that!(result).is_ok().subject;

        let expected_subgraphs = BTreeMap::from_iter([
            (
                sdl_subgraph_name.clone(),
                FullyResolvedSubgraph::builder()
                    .schema(sdl_subgraph_scenario.sdl.clone())
                    .is_fed_two(
                        sdl_subgraph_scenario
                            .subgraph_federation_version
                            .is_fed_two(),
                    )
                    .build(),
            ),
            (
                file_subgraph_name.clone(),
                FullyResolvedSubgraph::builder()
                    .routing_url(file_subgraph_scenario.routing_url.clone())
                    .schema(file_subgraph_scenario.sdl.clone())
                    .is_fed_two(
                        file_subgraph_scenario
                            .subgraph_federation_version
                            .is_fed_two(),
                    )
                    .build(),
            ),
            (
                remote_subgraph_name.clone(),
                FullyResolvedSubgraph::builder()
                    .routing_url(remote_subgraph_routing_url.clone())
                    .schema(remote_subgraph_scenario.sdl.clone())
                    .is_fed_two(
                        remote_subgraph_scenario
                            .subgraph_federation_version
                            .is_fed_two(),
                    )
                    .build(),
            ),
            (
                introspect_subgraph_name.clone(),
                FullyResolvedSubgraph::builder()
                    .routing_url(introspect_subgraph_routing_url.clone())
                    .schema(introspect_subgraph_scenario.sdl.clone())
                    .is_fed_two(
                        introspect_subgraph_scenario
                            .subgraph_federation_version
                            .is_fed_two(),
                    )
                    .build(),
            ),
        ]);
        assert_that!(resolved_supergraph_config.subgraphs()).is_equal_to(&expected_subgraphs);

        assert_that!(resolved_supergraph_config.federation_version())
            .is_equal_to(&expected_federation_version);

        Ok(())
    }

    #[rstest]
    // All subgraphs are fed two
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::Two),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::Two
        )
    )]
    // One subgraph is fed two
    #[case(
        sdl_subgraph_scenario(sdl(), subgraph_name(), SubgraphFederationVersion::Two),
        remote_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        introspect_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        ),
        file_subgraph_scenario(
            sdl(),
            subgraph_name(),
            routing_url(),
            SubgraphFederationVersion::One
        )
    )]
    #[tokio::test]
    async fn test_fully_resolve_subgraphs_error(
        supergraph_config_root_dir: TempDir,
        #[case] sdl_subgraph_scenario: SdlSubgraphScenario,
        #[case] remote_subgraph_scenario: RemoteSubgraphScenario,
        #[case] introspect_subgraph_scenario: IntrospectSubgraphScenario,
        #[case] file_subgraph_scenario: FileSubgraphScenario,
        // this version is expected to have been resolved from local and remote supergraph resolution steps already
    ) -> Result<()> {
        let target_federation_version = FederationVersion::LatestFedOne;
        file_subgraph_scenario.write_schema_file(supergraph_config_root_dir.path())?;
        let mut unresolved_subgraphs = BTreeMap::new();
        let sdl_subgraph_name = "sdl_subgraph".to_string();
        unresolved_subgraphs.insert(
            sdl_subgraph_name.clone(),
            sdl_subgraph_scenario.unresolved_subgraph,
        );
        let remote_subgraph_name = "remote_subgraph".to_string();
        unresolved_subgraphs.insert(
            remote_subgraph_name.clone(),
            remote_subgraph_scenario.unresolved_subgraph,
        );
        let introspect_subgraph_name = "introspect_subgraph".to_string();
        unresolved_subgraphs.insert(
            introspect_subgraph_name.clone(),
            introspect_subgraph_scenario.unresolved_subgraph,
        );
        let file_subgraph_name = "file_subgraph".to_string();
        unresolved_subgraphs.insert(
            file_subgraph_name.clone(),
            file_subgraph_scenario.unresolved_subgraph,
        );

        let unresolved_supergraph_config = UnresolvedSupergraphConfig {
            origin_path: None,
            subgraphs: unresolved_subgraphs,
            federation_version: Some(target_federation_version.clone()),
        };

        let RemoteSubgraphScenario {
            sdl: ref remote_subgraph_sdl,
            graph_ref: remote_subgraph_graph_ref,
            subgraph_name: remote_subgraph_subgraph_name,
            routing_url: remote_subgraph_routing_url,
            ..
        } = remote_subgraph_scenario;

        let mut mock_fetch_remote_subgraph = MockFetchRemoteSubgraph::new();
        mock_fetch_remote_subgraph
            .expect_fetch_remote_subgraph()
            .times(1)
            .with(
                predicate::eq(remote_subgraph_graph_ref.clone()),
                predicate::eq(remote_subgraph_subgraph_name.to_string()),
            )
            .returning({
                let remote_subgraph_sdl = remote_subgraph_sdl.to_string();
                let remote_subgraph_routing_url = remote_subgraph_routing_url.to_string();
                move |_, name| {
                    Ok(RemoteSubgraph::builder()
                        .name(name.to_string())
                        .routing_url(remote_subgraph_routing_url.to_string())
                        .schema(remote_subgraph_sdl.to_string())
                        .build())
                }
            });

        let IntrospectSubgraphScenario {
            sdl: ref introspect_subgraph_sdl,
            routing_url: introspect_subgraph_routing_url,
            introspection_headers: introspect_subgraph_introspection_headers,
            ..
        } = introspect_subgraph_scenario;

        let mut mock_introspect_subgraph = MockIntrospectSubgraph::new();
        mock_introspect_subgraph
            .expect_introspect_subgraph()
            .times(1)
            .with(
                predicate::eq(url::Url::from_str(&introspect_subgraph_routing_url)?),
                predicate::eq(introspect_subgraph_introspection_headers),
            )
            .returning({
                let introspect_subgraph_sdl = introspect_subgraph_sdl.to_string();
                move |_, _| Ok(introspect_subgraph_sdl.to_string())
            });

        let result = FullyResolvedSupergraphConfig::resolve(
            &mock_introspect_subgraph,
            &mock_fetch_remote_subgraph,
            Some(
                &Utf8PathBuf::from_path_buf(supergraph_config_root_dir.path().to_path_buf())
                    .unwrap(),
            ),
            unresolved_supergraph_config,
        )
        .await;

        mock_fetch_remote_subgraph.checkpoint();
        mock_introspect_subgraph.checkpoint();

        let mut fed_two_subgraph_names = HashSet::new();
        if sdl_subgraph_scenario
            .subgraph_federation_version
            .is_fed_two()
        {
            fed_two_subgraph_names.insert(sdl_subgraph_name);
        }
        if file_subgraph_scenario
            .subgraph_federation_version
            .is_fed_two()
        {
            fed_two_subgraph_names.insert(file_subgraph_name);
        }
        if remote_subgraph_scenario
            .subgraph_federation_version
            .is_fed_two()
        {
            fed_two_subgraph_names.insert(remote_subgraph_name);
        }
        if introspect_subgraph_scenario
            .subgraph_federation_version
            .is_fed_two()
        {
            fed_two_subgraph_names.insert(introspect_subgraph_name);
        }

        let err = assert_that!(result).is_err().subject;
        if let ResolveSupergraphConfigError::FederationVersionMismatch {
            specified_federation_version,
            subgraph_names,
        } = err
        {
            let subgraph_names = HashSet::from_iter(subgraph_names.iter().cloned());
            assert_that!(specified_federation_version).is_equal_to(&target_federation_version);
            assert_that!(subgraph_names).is_equal_to(&fed_two_subgraph_names);
        } else {
            panic!("Result contains the wrong type of error: {:?}", err);
        }

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn lazily_resolve_subgraphs(
        supergraph_config_root_dir: TempDir,
        sdl_subgraph_scenario: SdlSubgraphScenario,
        remote_subgraph_scenario: RemoteSubgraphScenario,
        introspect_subgraph_scenario: IntrospectSubgraphScenario,
        file_subgraph_scenario: FileSubgraphScenario,
    ) -> Result<()> {
        let supergraph_config_root_dir_path =
            Utf8PathBuf::from_path_buf(supergraph_config_root_dir.to_path_buf()).unwrap();

        let supergraph_config_origin_path = supergraph_config_root_dir_path.join("supergraph.yaml");

        file_subgraph_scenario.write_schema_file(supergraph_config_root_dir.path())?;
        let mut unresolved_subgraphs = BTreeMap::new();
        let sdl_subgraph_name = "sdl_subgraph".to_string();
        unresolved_subgraphs.insert(
            sdl_subgraph_name.clone(),
            sdl_subgraph_scenario.unresolved_subgraph,
        );
        let remote_subgraph_name = "remote_subgraph".to_string();
        unresolved_subgraphs.insert(
            remote_subgraph_name.clone(),
            remote_subgraph_scenario.unresolved_subgraph,
        );
        let introspect_subgraph_name = "introspect_subgraph".to_string();
        unresolved_subgraphs.insert(
            introspect_subgraph_name.clone(),
            introspect_subgraph_scenario.unresolved_subgraph,
        );
        let file_subgraph_name = "file_subgraph".to_string();
        unresolved_subgraphs.insert(
            file_subgraph_name.clone(),
            file_subgraph_scenario.unresolved_subgraph,
        );

        let unresolved_supergraph_config = UnresolvedSupergraphConfig {
            origin_path: Some(supergraph_config_origin_path),
            subgraphs: unresolved_subgraphs,
            federation_version: None,
        };

        let result = LazilyResolvedSupergraphConfig::resolve(
            &Utf8PathBuf::from_path_buf(supergraph_config_root_dir.path().to_path_buf()).unwrap(),
            unresolved_supergraph_config,
        )
        .await;
        let resolved_supergraph_config = assert_that!(result).is_ok().subject;
        // fed version is the default, since none provided
        assert_that!(resolved_supergraph_config.federation_version().as_ref()).is_none();

        let expected_subgraphs = BTreeMap::from_iter([
            (
                sdl_subgraph_name.clone(),
                LazilyResolvedSubgraph::builder()
                    .schema(SchemaSource::Sdl {
                        sdl: sdl_subgraph_scenario.sdl.clone(),
                    })
                    .build(),
            ),
            (
                file_subgraph_name.clone(),
                LazilyResolvedSubgraph::builder()
                    .schema(SchemaSource::File {
                        file: supergraph_config_root_dir_path
                            .join(file_subgraph_scenario.schema_file_path)
                            .canonicalize_utf8()?,
                    })
                    .routing_url(file_subgraph_scenario.routing_url)
                    .build(),
            ),
            (
                remote_subgraph_name.clone(),
                LazilyResolvedSubgraph::builder()
                    .schema(SchemaSource::Subgraph {
                        graphref: remote_subgraph_scenario.graph_ref.to_string(),
                        subgraph: remote_subgraph_scenario.subgraph_name.clone(),
                    })
                    .routing_url(remote_subgraph_scenario.routing_url.clone())
                    .build(),
            ),
            (
                introspect_subgraph_name.clone(),
                LazilyResolvedSubgraph::builder()
                    .schema(SchemaSource::SubgraphIntrospection {
                        subgraph_url: url::Url::from_str(
                            &introspect_subgraph_scenario.routing_url,
                        )?,
                        introspection_headers: Some(
                            introspect_subgraph_scenario.introspection_headers.clone(),
                        ),
                    })
                    .routing_url(introspect_subgraph_scenario.routing_url.clone())
                    .build(),
            ),
        ]);

        assert_that!(resolved_supergraph_config.subgraphs()).is_equal_to(&expected_subgraphs);

        Ok(())
    }
}
