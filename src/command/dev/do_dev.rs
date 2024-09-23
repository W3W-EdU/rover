use camino::Utf8PathBuf;
use futures::channel::mpsc::channel;
use futures::future::join_all;
use futures::stream::StreamExt;
use futures::FutureExt;
use rover_std::warnln;
use tokio::sync::mpsc;

use super::router::RouterConfigHandler;
use super::Dev;
use crate::command::dev::orchestrator::Orchestrator;
use crate::federation::supergraph_config::{get_supergraph_config, resolve_supergraph_config};
use crate::federation::Composer;
use crate::utils::client::StudioClientConfig;
use crate::{RoverError, RoverResult};

pub fn log_err_and_continue(err: RoverError) -> RoverError {
    let _ = err.print();
    err
}

impl Dev {
    pub async fn run(
        &self,
        override_install_path: Option<Utf8PathBuf>,
        client_config: StudioClientConfig,
    ) -> RoverResult<()> {
        self.opts
            .plugin_opts
            .prompt_for_license_accept(&client_config)?;

        let router_config_handler = RouterConfigHandler::try_from(&self.opts.supergraph_opts)?;
        let router_address = router_config_handler.get_router_address();
        let (subgraph_update_tx, subgraph_update_rx) = mpsc::channel(1);

        let supergraph_config = get_supergraph_config(
            &self.opts.supergraph_opts.graph_ref,
            self.opts.supergraph_opts.supergraph_config_path.as_ref(),
            self.opts.supergraph_opts.federation_version.as_ref(),
            client_config.clone(),
            &self.opts.plugin_opts.profile,
        )
        .await?;
        let supergraph_config = if let Some(supergraph_config) = supergraph_config {
            supergraph_config
        } else {
            self.opts
                .subgraph_opts
                .get_single_subgraph_from_opts(router_address)?
        };

        let resolved_supergraph_config = resolve_supergraph_config(
            supergraph_config.clone(),
            client_config.clone(),
            &self.opts.plugin_opts.profile,
        )
        .await?;
        let composer = Composer::new(
            resolved_supergraph_config,
            override_install_path.clone(),
            client_config.clone(),
            self.opts.plugin_opts.elv2_license_accepter,
            self.opts.plugin_opts.skip_update,
        )
        .await?;

        let mut orchestrator = Orchestrator::new(
            override_install_path,
            &client_config,
            subgraph_update_rx,
            self.opts.plugin_opts.clone(),
            composer,
            router_config_handler,
            self.opts.supergraph_opts.license.clone(),
        )
        .await?;
        warnln!(
            "Do not run this command in production! It is intended for local development only."
        );
        let (ready_sender, mut ready_receiver) = channel(1);

        let subgraph_watcher_handle = tokio::task::spawn(async move {
            orchestrator
                .receive_all_subgraph_updates(ready_sender)
                .await;
        });

        ready_receiver.next().await.unwrap();

        let subgraph_watchers = self
            .opts
            .supergraph_opts
            .get_subgraph_watchers(
                &client_config,
                supergraph_config,
                subgraph_update_tx.clone(),
                self.opts.subgraph_opts.subgraph_polling_interval,
                self.opts.subgraph_opts.subgraph_retries,
            )
            .await?;

        let futs = subgraph_watchers.into_iter().map(|mut watcher| async move {
            let _ = watcher
                .watch_subgraph_for_changes(client_config.retry_period)
                .await
                .map_err(log_err_and_continue);
        });
        tokio::join!(join_all(futs), subgraph_watcher_handle.map(|_| ()));
        Ok(())
    }
}
