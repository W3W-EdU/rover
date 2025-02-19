use std::{marker::Send, pin::Pin};

use apollo_federation_types::config::SchemaSource;
use futures::{Stream, StreamExt};
use tap::TapFallible;
use tokio::{sync::mpsc::UnboundedSender, task::AbortHandle};

use crate::{
    options::ProfileOpt, subtask::SubtaskHandleUnit, utils::client::StudioClientConfig, RoverError,
};

use super::{
    file::FileWatcher, introspection::SubgraphIntrospection, remote::RemoteSchema, sdl::Sdl,
};

#[derive(thiserror::Error, Debug)]
#[error("Unsupported subgraph introspection source: {:?}", .0)]
pub struct UnsupportedSchemaSource(SchemaSource);

/// A subgraph watcher watches subgraphs for changes. It's important to know when a subgraph
/// changes because it informs any listeners that they may need to react (eg, by recomposing when
/// the listener is composition)
#[derive(Debug, derive_getters::Getters)]
pub struct SubgraphWatcher {
    /// The kind of watcher used (eg, file, introspection)
    watcher: SubgraphWatcherKind,
}

#[derive(Debug, Clone)]
pub enum NonRepeatingFetch {
    RemoteSchema(RemoteSchema),
    Sdl(Sdl),
}

impl NonRepeatingFetch {
    pub async fn run(&self) -> Result<String, RoverError> {
        match self {
            Self::RemoteSchema(runner) => runner.run().await,
            Self::Sdl(runner) => Ok(runner.run()),
        }
    }
}

/// The kind of watcher attached to the subgraph. This may be either file watching, when we're
/// paying attention to a particular subgraph's SDL file, or introspection, when we get the SDL by
/// polling an endpoint that has introspection enabled
#[derive(Debug, Clone)]
pub enum SubgraphWatcherKind {
    /// Watch a file on disk.
    File(FileWatcher),
    /// Poll an endpoint via introspection.
    Introspect(SubgraphIntrospection),
    /// When there's an in-place change (eg, the SDL in the SupergraphConfig has changed or the
    /// SchemaSource::Subgraph now has a different subgraph name or points to a different
    /// GraphRef), we don't watch for changes: we either emit the changed SDL directly or call into
    /// Studio to get an updated SDL for the new GraphRef/subgraph combination
    Once(NonRepeatingFetch),
}

impl SubgraphWatcher {
    /// Derive the right SubgraphWatcher (ie, File, Introspection) from the federation-rs SchemaSource
    pub fn from_schema_source(
        schema_source: SchemaSource,
        profile: &ProfileOpt,
        client_config: &StudioClientConfig,
        introspection_polling_interval: u64,
    ) -> Result<Self, Box<UnsupportedSchemaSource>> {
        // SchemaSource comes from Apollo Federation types. Importantly, it strips comments and
        // directives from introspection (but not when the source is a file)
        match schema_source {
            SchemaSource::File { file } => Ok(Self {
                watcher: SubgraphWatcherKind::File(FileWatcher::new(file)),
            }),
            SchemaSource::SubgraphIntrospection {
                subgraph_url,
                introspection_headers,
            } => Ok(Self {
                watcher: SubgraphWatcherKind::Introspect(SubgraphIntrospection::new(
                    subgraph_url,
                    introspection_headers.map(|header_map| header_map.into_iter().collect()),
                    client_config,
                    introspection_polling_interval,
                )),
            }),
            SchemaSource::Subgraph { graphref, subgraph } => Ok(Self {
                watcher: SubgraphWatcherKind::Once(NonRepeatingFetch::RemoteSchema(
                    RemoteSchema::new(graphref, subgraph, profile, client_config),
                )),
            }),
            SchemaSource::Sdl { sdl } => Ok(Self {
                watcher: SubgraphWatcherKind::Once(NonRepeatingFetch::Sdl(Sdl::new(sdl))),
            }),
        }
    }
}

impl SubgraphWatcherKind {
    /// Watch a subgraph for changes based on the kind of watcher attached.
    ///
    /// Development note: this is a stream of Strings, but in the future we might want something
    /// more flexible to get type safety.
    async fn watch(&self) -> Pin<Box<dyn Stream<Item = String> + Send>> {
        match self {
            Self::File(file_watcher) => file_watcher.clone().watch(),
            Self::Introspect(introspection) => introspection.watch(),
            kind => unimplemented!("{kind:?} is not a watcher"),
        }
    }
}

/// A unit struct denoting a change to a subgraph, used by composition to know whether to
/// recompose.
#[derive(Debug, derive_getters::Getters)]
pub struct WatchedSdlChange {
    sdl: String,
}

impl SubtaskHandleUnit for SubgraphWatcher {
    type Output = WatchedSdlChange;

    fn handle(self, sender: UnboundedSender<Self::Output>) -> AbortHandle {
        tokio::spawn(async move {
            let mut watcher = self.watcher.watch().await;
            while let Some(sdl) = watcher.next().await {
                let _ = sender
                    .send(WatchedSdlChange { sdl })
                    .tap_err(|err| tracing::error!("{:?}", err));
            }
        })
        .abort_handle()
    }
}
