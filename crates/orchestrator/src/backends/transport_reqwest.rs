//! A real [`HttpTransport`] over `reqwest`, behind the off-by-default
//! `real-transport` Cargo feature so the default build (and every test in
//! this crate, which only ever constructs [`super::MockTransport`]) never
//! has to compile `reqwest`. Mirrors `crates/action`'s `real-input`
//! feature, which gates the real Windows `SendInput` backend the same way.
//!
//! Not wired into any [`super::HttpBackend`] constructor by default;
//! choosing this transport over `MockTransport` outside tests is
//! configuration-layer work for whichever lane owns backend setup.

use futures::future::BoxFuture;
use futures::stream::StreamExt;
use futures::FutureExt;
use reqwest::Client;

use super::error::TransportError;
use super::transport::{HttpMethod, HttpRequest, HttpResponse, HttpTransport};

pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport for ReqwestTransport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'static, Result<HttpResponse, TransportError>> {
        let client = self.client.clone();
        async move {
            let mut builder = match request.method {
                HttpMethod::Get => client.get(&request.url),
                HttpMethod::Post => client.post(&request.url),
            };
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            if !request.body.is_empty() {
                builder = builder.body(request.body);
            }

            let response = builder
                .send()
                .await
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            let status = response.status().as_u16();
            let body = response
                .bytes_stream()
                .map(|chunk| {
                    chunk
                        .map(|b| b.to_vec())
                        .map_err(|e| TransportError::Other(e.to_string()))
                })
                .boxed();
            Ok(HttpResponse::new(status, body))
        }
        .boxed()
    }
}
