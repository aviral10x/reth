//! Trait abstractions used by the payload crate.

use crate::{error::PayloadBuilderError, BuiltPayload};
use reth_rpc_types::engine::PayloadId;
use std::{future::Future, sync::Arc};
use tokio::sync::oneshot;

/// A type that can build a payload.
///
/// This type is a [`Future`] that resolves when the job is done (e.g. complete, timed out) or it
/// failed. It's not supposed to return the best payload built when it resolves, instead
/// [`PayloadJob::best_payload`] should be used for that.
///
/// A `PayloadJob` must always be prepared to return the best payload built so far to ensure there
/// is a valid payload to deliver to the CL, so it does not miss a slot, even if the payload is
/// empty.
///
/// Note: A `PayloadJob` need to be cancel safe because it might be dropped after the CL has requested the payload via `engine_getPayloadV1` (see also [engine API docs](https://github.com/ethereum/execution-apis/blob/6709c2a795b707202e93c4f2867fa0bf2640a84f/src/engine/paris.md#engine_getpayloadv1))
pub trait PayloadJob: Future<Output = Result<(), PayloadBuilderError>> + Send + Sync {
    /// Represents the payload attributes type that is used to spawn this payload job.
    type PayloadAttributes: std::fmt::Debug;
    /// Represents the future that resolves the block that's returned to the CL.
    type ResolvePayloadFuture: Future<Output = Result<Arc<BuiltPayload>, PayloadBuilderError>>
        + Send
        + Sync
        + 'static;

    /// Returns the best payload that has been built so far.
    ///
    /// Note: This is never called by the CL.
    fn best_payload(&self) -> Result<Arc<BuiltPayload>, PayloadBuilderError>;

    /// Returns the payload attributes for the payload being built.
    fn payload_attributes(&self) -> Result<Self::PayloadAttributes, PayloadBuilderError>;

    /// Called when the payload is requested by the CL.
    ///
    /// This is invoked on [`engine_getPayloadV2`](https://github.com/ethereum/execution-apis/blob/main/src/engine/shanghai.md#engine_getpayloadv2) and [`engine_getPayloadV1`](https://github.com/ethereum/execution-apis/blob/main/src/engine/paris.md#engine_getpayloadv1).
    ///
    /// The timeout for returning the payload to the CL is 1s, thus the future returned should
    /// resolve in under 1 second.
    ///
    /// Ideally this is the best payload built so far, or an empty block without transactions, if
    /// nothing has been built yet.
    ///
    /// According to the spec:
    /// > Client software MAY stop the corresponding build process after serving this call.
    ///
    /// It is at the discretion of the implementer whether the build job should be kept alive or
    /// terminated.
    ///
    /// If this returns [`KeepPayloadJobAlive::Yes`], then the [`PayloadJob`] will be polled
    /// once more. If this returns [`KeepPayloadJobAlive::No`] then the [`PayloadJob`] will be
    /// dropped after this call.
    fn resolve(&mut self) -> (Self::ResolvePayloadFuture, KeepPayloadJobAlive);
}

/// This is a trait that a payload builder or handle can implement to retrieve information relevant
/// to each payload job.
#[async_trait::async_trait]
pub trait PayloadBuilderTrait {
    // TODO(rjected): use PayloadBuilderConfig type here which just contains the payload attr +
    // built payload associated types?
    /// The payload attributes type that is used to spawn this payload job.
    type PayloadAttributes: std::fmt::Debug;

    /// Resolves the payload job and returns the best payload that has been built so far.
    ///
    /// Note: depending on the installed [PayloadJobGenerator], this may or may not terminate the
    /// job, See [PayloadJob::resolve].
    async fn resolve(
        &self,
        id: PayloadId,
    ) -> Option<Result<Arc<BuiltPayload>, PayloadBuilderError>>;

    /// Returns the best payload for the given identifier that has been built so far.
    async fn best_payload(
        &self,
        id: PayloadId,
    ) -> Option<Result<Arc<BuiltPayload>, PayloadBuilderError>>;

    /// Returns the payload attributes associated with the given identifier.
    ///
    /// Note: this returns the attributes of the payload and does not resolve the job.
    async fn payload_attributes(
        &self,
        id: PayloadId,
    ) -> Option<Result<Self::PayloadAttributes, PayloadBuilderError>>;

    /// Sends a message to the service to start building a new payload for the given payload.
    ///
    /// This is the same as [PayloadBuilderTrait::new_payload] but does not wait for the result and
    /// returns the receiver instead
    fn send_new_payload(
        &self,
        attr: Self::PayloadAttributes,
    ) -> oneshot::Receiver<Result<PayloadId, PayloadBuilderError>>;

    /// Starts building a new payload for the given payload attributes.
    ///
    /// Returns the identifier of the payload.
    ///
    /// Note: if there's already payload in progress with same identifier, it will be returned.
    async fn new_payload(
        &self,
        attr: Self::PayloadAttributes,
    ) -> Result<PayloadId, PayloadBuilderError>;
}

/// Whether the payload job should be kept alive or terminated after the payload was requested by
/// the CL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepPayloadJobAlive {
    /// Keep the job alive.
    Yes,
    /// Terminate the job.
    No,
}

/// A type that knows how to create new jobs for creating payloads.
pub trait PayloadJobGenerator: Send + Sync {
    /// The type that manages the lifecycle of a payload.
    ///
    /// This type is a future that yields better payloads.
    type Job: PayloadJob;

    /// Creates the initial payload and a new [`PayloadJob`] that yields better payloads over time.
    ///
    /// This is called when the CL requests a new payload job via a fork choice update.
    ///
    /// # Note
    ///
    /// This is expected to initially build a new (empty) payload without transactions, so it can be
    /// returned directly.
    fn new_payload_job(
        &self,
        attr: <Self::Job as PayloadJob>::PayloadAttributes,
    ) -> Result<Self::Job, PayloadBuilderError>;
}
