// Copyright (c) Facebook, Inc. and its affiliates.
// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::Cow, sync::Arc};

use futures::{future, lock::Mutex};
use linera_base::{
    data_types::{ArithmeticError, BlockHeight, HashedBlob},
    identifiers::{BlobId, ChainId, MessageId},
};
use linera_chain::data_types::{
    Block, BlockProposal, Certificate, ExecutedBlock, HashedCertificateValue, LiteCertificate,
};
use linera_execution::{
    committee::ValidatorName, BytecodeLocation, Query, Response, UserApplicationDescription,
    UserApplicationId,
};
use linera_storage::Storage;
use linera_views::views::ViewError;
use lru::LruCache;
use rand::prelude::SliceRandom;
use thiserror::Error;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    data_types::{BlockHeightRange, ChainInfo, ChainInfoQuery, ChainInfoResponse},
    node::{LocalValidatorNode, NotificationStream},
    worker::{Notification, ValidatorWorker, WorkerError, WorkerState},
};

/// A local node with a single worker, typically used by clients.
pub struct LocalNode<S> {
    state: WorkerState<S>,
}

/// A client to a local node.
#[derive(Clone)]
pub struct LocalNodeClient<S> {
    // TODO this should probably be a RwLock — a lot of operations here are read-only, so
    // using a full Mutex is wasteful
    // Alternatively we could push the locks down further
    node: Arc<Mutex<LocalNode<S>>>,
}

/// Error type for the operations on a local node.
#[derive(Debug, Error)]
pub enum LocalNodeError {
    #[error(transparent)]
    ArithmeticError(#[from] ArithmeticError),

    #[error(transparent)]
    ViewError(#[from] linera_views::views::ViewError),

    #[error("Local node operation failed: {0}")]
    WorkerError(#[from] WorkerError),

    #[error(
        "Failed to download certificates and update local node to the next height \
         {target_next_block_height} of chain {chain_id:?}"
    )]
    CannotDownloadCertificates {
        chain_id: ChainId,
        target_next_block_height: BlockHeight,
    },

    #[error("Failed to read blob {blob_id:?} of chain {chain_id:?}")]
    CannotReadLocalBlob { chain_id: ChainId, blob_id: BlobId },

    #[error("The local node doesn't have an active chain {0:?}")]
    InactiveChain(ChainId),

    #[error("The chain info response received from the local node is invalid")]
    InvalidChainInfoResponse,
}

// TODO can we generalize this to `NetworkActions` and other collectables?
type Notifications = Vec<Notification>;

type ResultWithNotifications<T, E> = Result<WithNotifications<T>, WithNotifications<E>>;

struct WithNotifications<T> {
    value: T,
    notifications: Vec<Notification>,
}

impl<T> WithNotifications<T> {
    fn new(value: T, notifications: Notifications) -> Self {
        Self { value, notifications }
    }

    fn extend(self, sink: &mut impl Extend<Notification>) -> T {
        sink.extend(self.notifications);
        self.value
    }
}

impl<T, E> WithNotifications<Result<T, E>> {
    fn distribute<E_: From<E>>(self) -> Result<WithNotifications<T>, WithNotifications<E_>> {
        match self.value {
            Ok(x) => Ok(WithNotifications::new(x, self.notifications)),
            Err(e) => Err(WithNotifications::new(e.into(), self.notifications)),
        }
    }
}

impl<T> From<T> for WithNotifications<T> {
    fn from(value: T) -> Self {
        Self { value, notifications: Default::default() }
    }
}

trait ResultWithNotificationsExt {
    type Ok;
    type Err;
    fn try_extend(self, sink: &mut impl Extend<Notification>) -> Result<Self::Ok, WithNotifications<Self::Err>>;
    fn factor(self, sink: &mut impl Extend<Notification>) -> Result<Self::Ok, Self::Err>;
}

impl<T, E> ResultWithNotificationsExt for ResultWithNotifications<T, E> {
    type Ok = T;
    type Err = E;

    fn try_extend(self, sink: &mut impl Extend<Notification>) -> Result<T, WithNotifications<E>> {
        match self {
            Ok(x) => {
                sink.extend(x.notifications);
                Ok(x.value)
            }
            Err(e) => Err(e),
        }
    }

    fn factor(self, sink: &mut impl Extend<Notification>) -> Result<T, E> {
        match self {
            Ok(x) => Ok(x.extend(sink)),
            Err(e) => Err(e.extend(sink)),
        }
    }
}

impl<S> LocalNodeClient<S>
where
    S: Storage + Clone + Send + Sync + 'static,
    ViewError: From<S::ContextError>,
{
    pub async fn handle_block_proposal(
        &self,
        proposal: BlockProposal,
    ) -> Result<ChainInfoResponse, LocalNodeError> {
        let mut node = self.node.lock().await;
        // In local nodes, we can trust fully_handle_certificate to carry all actions eventually.
        let (response, _actions) = node.state.handle_block_proposal(proposal).await?;
        Ok(response)
    }

    pub async fn handle_lite_certificate(
        &mut self,
        certificate: LiteCertificate<'_>,
    ) -> ResultWithNotifications<ChainInfoResponse, LocalNodeError> {
        let mut node = self.node.lock().await;
        let mut notifications = Vec::new();
        let full_cert = node.state.full_certificate(certificate).await.map_err(LocalNodeError::from)?;
        let response = node
            .state
            .fully_handle_certificate_with_notifications(
                full_cert,
                vec![],
                vec![],
                Some(&mut notifications),
            )
            .await;
        WithNotifications::new(response, notifications).distribute()
    }

    pub async fn handle_certificate(
        &self,
        certificate: Certificate,
        hashed_certificate_values: Vec<HashedCertificateValue>,
        hashed_blobs: Vec<HashedBlob>,
    ) -> ResultWithNotifications<ChainInfoResponse, LocalNodeError> {
        let mut node = self.node.lock().await;
        let mut notifications = Vec::new();
        let response = node
            .state
            .fully_handle_certificate_with_notifications(
                certificate,
                hashed_certificate_values,
                hashed_blobs,
                Some(&mut notifications),
            )
            .await;
        WithNotifications::new(response, notifications).distribute()
    }

    pub async fn handle_chain_info_query(
        &self,
        query: ChainInfoQuery,
    ) -> Result<ChainInfoResponse, LocalNodeError> {
        let node = self.node.lock().await;
        // In local nodes, we can trust fully_handle_certificate to carry all actions eventually.
        let (response, _actions) = node.state.handle_chain_info_query(query).await?;
        Ok(response)
    }
}

impl<S> LocalNodeClient<S> {
    pub fn new(state: WorkerState<S>) -> Self {
        let node = LocalNode { state };

        Self {
            node: Arc::new(Mutex::new(node)),
        }
    }
}

impl<S> LocalNodeClient<S>
where
    S: Clone,
{
    pub(crate) async fn storage_client(&self) -> S {
        let node = self.node.lock().await;
        node.state.storage_client().clone()
    }
}

impl<S> LocalNodeClient<S>
where
    S: Storage + Clone + Send + Sync + 'static,
    ViewError: From<S::ContextError>,
{
    pub(crate) async fn stage_block_execution(
        &self,
        block: Block,
    ) -> Result<(ExecutedBlock, ChainInfoResponse), LocalNodeError> {
        let mut node = self.node.lock().await;
        let (executed_block, info) = node.state.stage_block_execution(block).await?;
        Ok((executed_block, info))
    }

    async fn find_missing_application_bytecodes<A>(
        &self,
        chain_id: ChainId,
        locations: &[BytecodeLocation],
        node: &mut A,
        name: ValidatorName,
    ) -> Vec<HashedCertificateValue>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        future::join_all(locations.iter().map(|location| {
            let mut node = node.clone();
            async move {
                Self::try_download_hashed_certificate_value_from(
                    name, &mut node, chain_id, *location,
                )
                    .await
            }
        }))
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    }

    async fn find_missing_blobs<A>(
        &self,
        chain_id: ChainId,
        blob_ids: &[BlobId],
        node: &mut A,
        name: ValidatorName,
    ) -> Vec<HashedBlob>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        future::join_all(blob_ids.iter().map(|blob_id| {
            let mut node = node.clone();
            async move { Self::try_download_blob_from(name, &mut node, chain_id, *blob_id).await }
        }))
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    }

    async fn try_process_certificates<A>(
        &self,
        name: ValidatorName,
        node: &mut A,
        chain_id: ChainId,
        certificates: Vec<Certificate>,
    ) -> WithNotifications<Option<Box<ChainInfo>>>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let mut info = None;
        let mut notifications = vec![];

        for certificate in certificates {
            let hash = certificate.hash();
            if !certificate.value().is_confirmed() || certificate.value().chain_id() != chain_id {
                // The certificate is not as expected. Give up.
                tracing::warn!("Failed to process network certificate {}", hash);
                return WithNotifications::new(info, notifications);
            }
            let result = self
                .handle_certificate(certificate.clone(), vec![], vec![])
                .await;

            let result = match result.factor(&mut notifications) {
                Err(LocalNodeError::WorkerError(WorkerError::ApplicationBytecodesNotFound(
                    locations,
                ))) => {
                    let values = self
                        .find_missing_application_bytecodes(
                            certificate.value().chain_id(),
                            &locations,
                            node,
                            name,
                        )
                        .await;

                    if values.len() != locations.len() {
                        Err(LocalNodeError::WorkerError(WorkerError::ApplicationBytecodesNotFound(
                            locations,
                        )))
                    } else {
                        self.handle_certificate(certificate, values, vec![]).await.factor(&mut notifications)
                    }
                }
                Err(LocalNodeError::WorkerError(WorkerError::BlobsNotFound(blob_ids))) => {
                    let blobs = self
                        .find_missing_blobs(certificate.value().chain_id(), &blob_ids, node, name)
                        .await;

                    if blobs.len() != blob_ids.len() {
                        Err(LocalNodeError::WorkerError(WorkerError::BlobsNotFound(blob_ids)))
                    } else {
                        self.handle_certificate(certificate, vec![], blobs).await.factor(&mut notifications)
                    }
                }
                Err(LocalNodeError::WorkerError(
                    WorkerError::ApplicationBytecodesAndBlobsNotFound(locations, blob_ids),
                )) => {
                    let chain_id = certificate.value().chain_id();
                    let values = self
                        .find_missing_application_bytecodes(chain_id, &locations, node, name)
                        .await;
                    let blobs = self
                        .find_missing_blobs(chain_id, &blob_ids, node, name)
                        .await;

                    if values.len() != locations.len() || blobs.len() != blob_ids.len() {
                        Err(LocalNodeError::WorkerError(
                            WorkerError::ApplicationBytecodesAndBlobsNotFound(locations, blob_ids),
                        ))
                    } else {
                        self.handle_certificate(certificate, values, blobs).await.factor(&mut notifications)
                    }
                }
                result => result,
            };

            match result {
                Ok(response) => info = Some(response.info),
                Err(error) => {
                    // The certificate is not as expected. Give up.
                    tracing::warn!("Failed to process network certificate {}: {}", hash, error);
                    return WithNotifications::new(info, notifications);
                }
            };
        }

        // Done with all certificates.
        WithNotifications::new(info, notifications)
    }

    pub(crate) async fn local_chain_info(
        &self,
        chain_id: ChainId,
    ) -> Result<Box<ChainInfo>, LocalNodeError> {
        let query = ChainInfoQuery::new(chain_id);
        Ok(self.handle_chain_info_query(query).await?.info)
    }

    pub async fn query_application(
        &self,
        chain_id: ChainId,
        query: Query,
    ) -> Result<Response, LocalNodeError> {
        let mut node = self.node.lock().await;
        let response = node.state.query_application(chain_id, query).await?;
        Ok(response)
    }

    pub async fn describe_application(
        &self,
        chain_id: ChainId,
        application_id: UserApplicationId,
    ) -> Result<UserApplicationDescription, LocalNodeError> {
        let mut node = self.node.lock().await;
        let response = node
            .state
            .describe_application(chain_id, application_id)
            .await?;
        Ok(response)
    }

    pub async fn recent_blob(&self, blob_id: &BlobId) -> Option<HashedBlob> {
        let mut node = self.node.lock().await;
        node.state.recent_blob(blob_id).await
    }

    pub async fn recent_hashed_blobs(
        &self,
    ) -> futures::lock::MappedMutexGuard<LocalNode<S>, LruCache<BlobId, HashedBlob>> {
        futures::lock::MutexGuard::map(self.node.lock().await, |node: &mut LocalNode<S>| node.state.recent_hashed_blobs_mut())
    }

    pub async fn cache_recent_blob(&self, hashed_blob: &HashedBlob) -> bool {
        let mut node = self.node.lock().await;
        node.state
            .cache_recent_blob(Cow::Borrowed(hashed_blob))
            .await
    }

    pub async fn download_certificates<A>(
        &self,
        mut validators: Vec<(ValidatorName, A)>,
        chain_id: ChainId,
        target_next_block_height: BlockHeight,
    ) -> Result<Box<ChainInfo>, LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        // Sequentially try each validator in random order.
        validators.shuffle(&mut rand::thread_rng());
        for (name, node) in validators {
            let info = self.local_chain_info(chain_id).await?;
            if target_next_block_height <= info.next_block_height {
                return Ok(info);
            }
            self.try_download_certificates_from(
                name,
                node,
                chain_id,
                info.next_block_height,
                target_next_block_height,
            )
                .await?;
        }
        let info = self.local_chain_info(chain_id).await?;
        if target_next_block_height <= info.next_block_height {
            Ok(info)
        } else {
            Err(LocalNodeError::CannotDownloadCertificates {
                chain_id,
                target_next_block_height,
            })
        }
    }

    /// Downloads and stores the specified hashed certificate values, unless they are already in the cache or storage.
    ///
    /// Does not fail if a hashed certificate value can't be downloaded; it just gets omitted from the result.
    pub async fn read_or_download_hashed_certificate_values<A>(
        &self,
        validators: Vec<(ValidatorName, A)>,
        hashed_certificate_value_locations: impl IntoIterator<Item = (BytecodeLocation, ChainId)>,
    ) -> Result<Vec<HashedCertificateValue>, LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let mut values = vec![];
        let mut tasks = vec![];
        let mut node = self.node.lock().await;
        for (location, chain_id) in hashed_certificate_value_locations {
            if let Some(value) = node
                .state
                .recent_hashed_certificate_value(&location.certificate_hash)
                .await
            {
                values.push(value);
            } else {
                let validators = validators.clone();
                let storage = node.state.storage_client().clone();
                tasks.push(Self::read_or_download_hashed_certificate_value(
                    storage, validators, chain_id, location,
                ));
            }
        }
        drop(node); // Free the lock while awaiting the tasks.
        if tasks.is_empty() {
            return Ok(values);
        }
        let results = future::join_all(tasks).await;
        let mut node = self.node.lock().await;
        for result in results {
            if let Some(value) = result? {
                node.state
                    .cache_recent_hashed_certificate_value(Cow::Borrowed(&value))
                    .await;
                values.push(value);
            }
        }
        Ok(values)
    }

    pub async fn read_or_download_hashed_certificate_value<A>(
        storage: S,
        validators: Vec<(ValidatorName, A)>,
        chain_id: ChainId,
        location: BytecodeLocation,
    ) -> Result<Option<HashedCertificateValue>, LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        match storage
            .read_hashed_certificate_value(location.certificate_hash)
            .await
        {
            Ok(hashed_certificate_value) => return Ok(Some(hashed_certificate_value)),
            Err(ViewError::NotFound(..)) => {}
            Err(err) => Err(err)?,
        }
        match Self::download_hashed_certificate_value(validators, chain_id, location).await {
            Some(hashed_certificate_value) => {
                storage
                    .write_hashed_certificate_value(&hashed_certificate_value)
                    .await?;
                Ok(Some(hashed_certificate_value))
            }
            None => Ok(None),
        }
    }

    /// Obtains the certificate containing the specified message.
    pub async fn certificate_for(
        &mut self,
        message_id: &MessageId,
    ) -> Result<Certificate, LocalNodeError> {
        let query = ChainInfoQuery::new(message_id.chain_id)
            .with_sent_certificates_in_range(BlockHeightRange::single(message_id.height));
        let info = self.handle_chain_info_query(query).await?.info;
        let certificate = info
            .requested_sent_certificates
            .into_iter()
            .find(|certificate| certificate.value().has_message(message_id))
            .ok_or_else(|| {
                ViewError::not_found("could not find certificate with message {}", message_id)
            })?;
        Ok(certificate)
    }

    async fn try_download_certificates_from<A>(
        &self,
        name: ValidatorName,
        mut node: A,
        chain_id: ChainId,
        mut start: BlockHeight,
        stop: BlockHeight,
    ) -> Result<(), LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let mut notifications = vec![];

        while start < stop {
            // TODO(#2045): Analyze network errors instead of guessing the batch size.
            let limit = u64::from(stop)
                .checked_sub(u64::from(start))
                .ok_or(ArithmeticError::Overflow)?
                .min(1000);
            let Some(certificates) = self
                .try_query_certificates_from(name, &mut node, chain_id, start, limit)
                .await?
                else {
                    break;
                };
            let Some(info) = self
                .try_process_certificates(name, &mut node, chain_id, certificates)
                .await
                .extend(&mut notifications)
                else {
                    break;
                };
            assert!(info.next_block_height > start);
            start = info.next_block_height;
        }
        Ok(())
    }

    async fn try_query_certificates_from<A>(
        &self,
        name: ValidatorName,
        node: &mut A,
        chain_id: ChainId,
        start: BlockHeight,
        limit: u64,
    ) -> Result<Option<Vec<Certificate>>, LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        tracing::debug!(?name, ?chain_id, ?start, ?limit, "Querying certificates");
        let range = BlockHeightRange {
            start,
            limit: Some(limit),
        };
        let query = ChainInfoQuery::new(chain_id).with_sent_certificates_in_range(range);
        if let Ok(response) = node.handle_chain_info_query(query).await {
            if response.check(name).is_err() {
                return Ok(None);
            }
            let ChainInfo {
                requested_sent_certificates,
                ..
            } = *response.info;
            Ok(Some(requested_sent_certificates))
        } else {
            Ok(None)
        }
    }

    pub async fn synchronize_chain_state<A>(
        &self,
        validators: Vec<(ValidatorName, A)>,
        chain_id: ChainId,
    ) -> Result<Box<ChainInfo>, LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let futures = validators
            .into_iter()
            .map(|(name, node)| {
                let mut client = self.clone();
                async move {
                    client
                        .try_synchronize_chain_state_from(name, node, chain_id)
                        .await
                }
            })
            .collect::<Vec<_>>();
        futures::future::join_all(futures).await;
        let info = self.local_chain_info(chain_id).await?;
        Ok(info)
    }

    pub async fn try_synchronize_chain_state_from<A>(
        &mut self,
        name: ValidatorName,
        mut node: A,
        chain_id: ChainId,
    ) -> ResultWithNotifications<(), LocalNodeError>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let mut notifications = vec![];
        let local_info = self.local_chain_info(chain_id).await?;
        let range = BlockHeightRange {
            start: local_info.next_block_height,
            limit: None,
        };
        let query = ChainInfoQuery::new(chain_id)
            .with_sent_certificates_in_range(range)
            .with_manager_values();
        let info = match node.handle_chain_info_query(query).await {
            Ok(response) if response.check(name).is_ok() => response.info,
            Ok(_) => {
                tracing::warn!("Ignoring invalid response from validator");
                // Give up on this validator.
                return Ok(WithNotifications::new((), notifications));
            }
            Err(err) => {
                tracing::warn!("Ignoring error from validator: {}", err);
                return Ok(WithNotifications::new((), notifications));
            }
        };
        if !info.requested_sent_certificates.is_empty()
            && self
                .try_process_certificates(
                    name,
                    &mut node,
                    chain_id,
                    info.requested_sent_certificates,
                )
                .await
                .extend(&mut notifications)
                .is_none()
        {
            return Ok(WithNotifications::new((), notifications));
        };
        if let Some(proposal) = info.manager.requested_proposed {
            if proposal.content.block.chain_id == chain_id {
                let owner = proposal.owner;
                if let Err(error) = self.handle_block_proposal(*proposal).await {
                    tracing::warn!("Skipping proposal from {}: {}", owner, error);
                }
            }
        }
        if let Some(cert) = info.manager.requested_locked {
            if cert.value().is_validated() && cert.value().chain_id() == chain_id {
                let hash = cert.hash();
                if let Err(error) = self.handle_certificate(*cert, vec![], vec![]).await {
                    tracing::warn!("Skipping certificate {}: {}", hash, error.value);
                }
            }
        }
        Ok(WithNotifications::new((), notifications))
    }

    pub async fn download_hashed_certificate_value<A>(
        mut validators: Vec<(ValidatorName, A)>,
        chain_id: ChainId,
        location: BytecodeLocation,
    ) -> Option<HashedCertificateValue>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        // Sequentially try each validator in random order.
        validators.shuffle(&mut rand::thread_rng());
        for (name, mut node) in validators {
            if let Some(value) = Self::try_download_hashed_certificate_value_from(
                name, &mut node, chain_id, location,
            )
                .await
            {
                return Some(value);
            }
        }
        None
    }

    pub async fn download_blob<A>(
        mut validators: Vec<(ValidatorName, A)>,
        chain_id: ChainId,
        blob_id: BlobId,
    ) -> Option<HashedBlob>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        // Sequentially try each validator in random order.
        validators.shuffle(&mut rand::thread_rng());
        for (name, mut node) in validators {
            if let Some(blob) =
                Self::try_download_blob_from(name, &mut node, chain_id, blob_id).await
            {
                return Some(blob);
            }
        }
        None
    }

    async fn try_download_blob_from<A>(
        name: ValidatorName,
        node: &mut A,
        chain_id: ChainId,
        blob_id: BlobId,
    ) -> Option<HashedBlob>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let query = ChainInfoQuery::new(chain_id).with_blob(blob_id);
        if let Ok(response) = node.handle_chain_info_query(query).await {
            if response.check(name).is_ok() {
                return response.info.requested_blob;
            }
        }
        None
    }

    async fn try_download_hashed_certificate_value_from<A>(
        name: ValidatorName,
        node: &mut A,
        chain_id: ChainId,
        location: BytecodeLocation,
    ) -> Option<HashedCertificateValue>
    where
        A: LocalValidatorNode + Clone + 'static,
    {
        let query =
            ChainInfoQuery::new(chain_id).with_hashed_certificate_value(location.certificate_hash);
        if let Ok(response) = node.handle_chain_info_query(query).await {
            if response.check(name).is_ok() {
                return response.info.requested_hashed_certificate_value;
            }
        }
        None
    }
}
