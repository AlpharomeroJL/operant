//! The injectable HTTP seam.
//!
//! Every [`super::ModelBackend`] built on the OpenAI-compatible client
//! speaks to the network exclusively through [`HttpTransport`], so tests
//! (and CI, and air-gapped installs) swap in [`MockTransport`] and never
//! open a real socket. A real transport lives behind the off-by-default
//! `real-transport` feature in `transport_reqwest.rs`.
//!
//! `send` resolves as soon as the response's status is known and hands back
//! the body as its own stream of chunks, deliberately mirroring how a real
//! HTTP client works (status/headers arrive, then the body streams in over
//! time). A transport that buffered the whole body before resolving would
//! turn every backend's `complete()` into "wait for the entire generation,
//! then replay it fast", which defeats the point of `complete()` returning
//! a stream at all.

use std::collections::VecDeque;
use std::sync::Mutex;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream};
use futures::{FutureExt, StreamExt};

use super::error::TransportError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn post(url: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: Vec::new(),
            body,
        }
    }

    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// One chunk of a response body as it arrives.
pub type BodyChunk = Result<Vec<u8>, TransportError>;

/// A response body as a live stream of chunks, in wire order.
pub type BodyStream = BoxStream<'static, BodyChunk>;

/// An HTTP response: the status, known as soon as `send` resolves, plus the
/// body as a [`BodyStream`] the caller pulls at its own pace.
pub struct HttpResponse {
    pub status: u16,
    pub body: BodyStream,
}

/// Manual: `body` is a `BoxStream`, which does not implement `Debug`.
impl std::fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl HttpResponse {
    pub fn new(status: u16, body: BodyStream) -> Self {
        Self { status, body }
    }

    /// Drain the body stream and concatenate it, for callers (error paths,
    /// tests) that want the whole thing at once. Not used on the
    /// happy-path streaming route.
    pub async fn collect_body(mut self) -> Result<Vec<u8>, TransportError> {
        use futures::StreamExt;
        let mut buf = Vec::new();
        while let Some(chunk) = self.body.next().await {
            buf.extend_from_slice(&chunk?);
        }
        Ok(buf)
    }
}

/// The seam every backend HTTP call goes through. Implement this to add a
/// transport (see `transport_reqwest` behind the `real-transport` feature);
/// every test in this crate only ever constructs [`MockTransport`].
pub trait HttpTransport: Send + Sync {
    fn send(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'static, Result<HttpResponse, TransportError>>;
}

#[derive(Clone)]
enum MockOutcome {
    Ok { status: u16, chunks: Vec<Vec<u8>> },
    Err(TransportError),
}

#[derive(Default)]
struct MockState {
    queue: VecDeque<MockOutcome>,
    requests: Vec<HttpRequest>,
}

/// A canned, in-memory [`HttpTransport`]. Queue responses (or errors) with
/// [`MockTransport::push_body`] / [`MockTransport::push_chunks`] /
/// [`MockTransport::push_error`]; `send` consumes them in FIFO order and
/// records every request it received, so contract tests can assert on the
/// exact URL, headers, and body a quirk-table entry produced. Queued
/// chunks are replayed as separate `BodyStream` items in order, so a test
/// can deliberately split one SSE frame across chunk boundaries.
#[derive(Default)]
pub struct MockTransport {
    state: Mutex<MockState>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MockState::default()),
        }
    }

    /// Queue a 200 OK response built from a single body chunk.
    pub fn push_body(&self, body: impl Into<Vec<u8>>) {
        self.push_chunks(vec![body.into()]);
    }

    /// Queue a 200 OK response delivered as several chunks, in order. Use
    /// this to prove a dialect parser survives a frame split across chunk
    /// boundaries.
    pub fn push_chunks(&self, chunks: Vec<Vec<u8>>) {
        self.push_status_chunks(200, chunks);
    }

    /// Queue a response with an explicit (possibly non-2xx) status.
    pub fn push_status_chunks(&self, status: u16, chunks: Vec<Vec<u8>>) {
        self.state
            .lock()
            .unwrap()
            .queue
            .push_back(MockOutcome::Ok { status, chunks });
    }

    /// Queue a transport-level failure (no HTTP response at all).
    pub fn push_error(&self, err: TransportError) {
        self.state
            .lock()
            .unwrap()
            .queue
            .push_back(MockOutcome::Err(err));
    }

    /// Every request received so far, oldest first.
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.state.lock().unwrap().requests.clone()
    }

    /// The most recent request received, if any.
    pub fn last_request(&self) -> Option<HttpRequest> {
        self.state.lock().unwrap().requests.last().cloned()
    }
}

impl HttpTransport for MockTransport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'static, Result<HttpResponse, TransportError>> {
        let outcome = {
            let mut state = self.state.lock().unwrap();
            state.requests.push(request);
            state.queue.pop_front()
        };
        async move {
            match outcome {
                Some(MockOutcome::Ok { status, chunks }) => {
                    let body = stream::iter(chunks.into_iter().map(Ok)).boxed();
                    Ok(HttpResponse::new(status, body))
                }
                Some(MockOutcome::Err(e)) => Err(e),
                None => Err(TransportError::Other(
                    "MockTransport: no canned response queued for this call".to_string(),
                )),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn mock_transport_replays_queued_responses_in_order() {
        let mock = MockTransport::new();
        mock.push_body(b"first".to_vec());
        mock.push_body(b"second".to_vec());

        let r1 = mock
            .send(HttpRequest::post("http://x/1", vec![]))
            .await
            .unwrap();
        let body1 = r1.collect_body().await.unwrap();
        let r2 = mock
            .send(HttpRequest::post("http://x/2", vec![]))
            .await
            .unwrap();
        let body2 = r2.collect_body().await.unwrap();

        assert_eq!(body1, b"first");
        assert_eq!(body2, b"second");
        assert_eq!(mock.requests().len(), 2);
        assert_eq!(mock.last_request().unwrap().url, "http://x/2");
    }

    #[tokio::test]
    async fn mock_transport_delivers_queued_chunks_as_separate_stream_items() {
        let mock = MockTransport::new();
        mock.push_chunks(vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        let response = mock
            .send(HttpRequest::post("http://x", vec![]))
            .await
            .unwrap();
        let chunks: Vec<Vec<u8>> = response.body.map(|c| c.unwrap()).collect().await;
        assert_eq!(chunks, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[tokio::test]
    async fn mock_transport_errors_when_queue_is_empty() {
        let mock = MockTransport::new();
        let err = mock
            .send(HttpRequest::post("http://x", vec![]))
            .await
            .unwrap_err();
        assert!(matches!(err, TransportError::Other(_)));
        // The request is still recorded even though no response was queued.
        assert_eq!(mock.requests().len(), 1);
    }

    #[tokio::test]
    async fn mock_transport_replays_queued_transport_errors() {
        let mock = MockTransport::new();
        mock.push_error(TransportError::Timeout(5000));
        let err = mock
            .send(HttpRequest::post("http://x", vec![]))
            .await
            .unwrap_err();
        assert_eq!(err, TransportError::Timeout(5000));
    }

    #[test]
    fn header_value_lookup_is_case_insensitive() {
        let req = HttpRequest::post("http://x", vec![]).header("X-Api-Key", "secret");
        assert_eq!(req.header_value("x-api-key"), Some("secret"));
        assert_eq!(req.header_value("X-API-KEY"), Some("secret"));
        assert_eq!(req.header_value("missing"), None);
    }
}
