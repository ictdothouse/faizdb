//! Generated-style Prost Message Structs & Tonic Server Service Definitions for FaizDB.

use tonic::codegen::*;

// ── Message Structs ───────────────────────────────────────────────

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryRequest {
    #[prost(string, tag = "1")]
    pub query: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub database: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub token: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,
    #[prost(string, tag = "2")]
    pub result_json: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub execution_time_us: u64,
    #[prost(string, tag = "4")]
    pub error_message: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VectorSearchRequest {
    #[prost(string, tag = "1")]
    pub collection: ::prost::alloc::string::String,
    #[prost(float, repeated, tag = "2")]
    pub vector: ::prost::alloc::vec::Vec<f32>,
    #[prost(uint32, tag = "3")]
    pub top_k: u32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VectorHit {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(float, tag = "2")]
    pub score: f32,
    #[prost(string, tag = "3")]
    pub document_json: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VectorSearchResponse {
    #[prost(message, repeated, tag = "1")]
    pub hits: ::prost::alloc::vec::Vec<VectorHit>,
    #[prost(uint64, tag = "2")]
    pub search_time_us: u64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InsertRequest {
    #[prost(string, tag = "1")]
    pub collection: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "2")]
    pub documents_json: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InsertResponse {
    #[prost(uint32, tag = "1")]
    pub inserted_count: u32,
    #[prost(string, repeated, tag = "2")]
    pub inserted_ids: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StreamRequest {
    #[prost(string, tag = "1")]
    pub collection: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChangeEventMsg {
    #[prost(string, tag = "1")]
    pub event_type: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub collection: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub document_id: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub payload_json: ::prost::alloc::string::String,
    #[prost(uint64, tag = "5")]
    pub timestamp_ms: u64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HealthRequest {
    #[prost(string, tag = "1")]
    pub service: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HealthResponse {
    #[prost(string, tag = "1")]
    pub status: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub uptime_seconds: u64,
}

// ── Tonic Server Trait & Dispatcher ───────────────────────────────

#[tonic::async_trait]
pub trait FaizDbService: Send + Sync + 'static {
    type SubscribeChangeStreamStream: tonic::codegen::tokio_stream::Stream<
            Item = std::result::Result<ChangeEventMsg, tonic::Status>,
        > + Send
        + 'static;

    async fn execute_query(
        &self,
        request: tonic::Request<QueryRequest>,
    ) -> std::result::Result<tonic::Response<QueryResponse>, tonic::Status>;

    async fn vector_search(
        &self,
        request: tonic::Request<VectorSearchRequest>,
    ) -> std::result::Result<tonic::Response<VectorSearchResponse>, tonic::Status>;

    async fn insert_documents(
        &self,
        request: tonic::Request<InsertRequest>,
    ) -> std::result::Result<tonic::Response<InsertResponse>, tonic::Status>;

    async fn subscribe_change_stream(
        &self,
        request: tonic::Request<StreamRequest>,
    ) -> std::result::Result<tonic::Response<Self::SubscribeChangeStreamStream>, tonic::Status>;

    async fn health_check(
        &self,
        request: tonic::Request<HealthRequest>,
    ) -> std::result::Result<tonic::Response<HealthResponse>, tonic::Status>;
}

/// Generated server implementation for `faizdb.v1.FaizDbService`
#[derive(Debug)]
pub struct FaizDbServiceServer<T: FaizDbService> {
    inner: Arc<T>,
    accept_compression_encodings: EnabledCompressionEncodings,
    send_compression_encodings: EnabledCompressionEncodings,
    max_decoding_message_size: Option<usize>,
    max_encoding_message_size: Option<usize>,
}

impl<T: FaizDbService> FaizDbServiceServer<T> {
    pub fn new(inner: T) -> Self {
        Self::from_arc(Arc::new(inner))
    }

    pub fn from_arc(inner: Arc<T>) -> Self {
        Self {
            inner,
            accept_compression_encodings: Default::default(),
            send_compression_encodings: Default::default(),
            max_decoding_message_size: None,
            max_encoding_message_size: None,
        }
    }
}

impl<T, B> tonic::codegen::Service<http::Request<B>> for FaizDbServiceServer<T>
where
    T: FaizDbService,
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let inner = self.inner.clone();

        match req.uri().path() {
            "/faizdb.v1.FaizDbService/ExecuteQuery" => {
                struct ExecuteQuerySvc<T: FaizDbService>(pub Arc<T>);
                impl<T: FaizDbService> tonic::server::UnaryService<QueryRequest> for ExecuteQuerySvc<T> {
                    type Response = QueryResponse;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                    fn call(&mut self, request: tonic::Request<QueryRequest>) -> Self::Future {
                        let inner = self.0.clone();
                        let fut = async move { T::execute_query(&inner, request).await };
                        Box::pin(fut)
                    }
                }
                let method = ExecuteQuerySvc(inner);
                let codec = tonic::codec::ProstCodec::default();
                let mut grpc = tonic::server::Grpc::new(codec);
                Box::pin(async move {
                    let res = grpc.unary(method, req).await;
                    Ok(res)
                })
            }
            "/faizdb.v1.FaizDbService/VectorSearch" => {
                struct VectorSearchSvc<T: FaizDbService>(pub Arc<T>);
                impl<T: FaizDbService> tonic::server::UnaryService<VectorSearchRequest> for VectorSearchSvc<T> {
                    type Response = VectorSearchResponse;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                    fn call(
                        &mut self,
                        request: tonic::Request<VectorSearchRequest>,
                    ) -> Self::Future {
                        let inner = self.0.clone();
                        let fut = async move { T::vector_search(&inner, request).await };
                        Box::pin(fut)
                    }
                }
                let method = VectorSearchSvc(inner);
                let codec = tonic::codec::ProstCodec::default();
                let mut grpc = tonic::server::Grpc::new(codec);
                Box::pin(async move {
                    let res = grpc.unary(method, req).await;
                    Ok(res)
                })
            }
            "/faizdb.v1.FaizDbService/InsertDocuments" => {
                struct InsertDocumentsSvc<T: FaizDbService>(pub Arc<T>);
                impl<T: FaizDbService> tonic::server::UnaryService<InsertRequest> for InsertDocumentsSvc<T> {
                    type Response = InsertResponse;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                    fn call(&mut self, request: tonic::Request<InsertRequest>) -> Self::Future {
                        let inner = self.0.clone();
                        let fut = async move { T::insert_documents(&inner, request).await };
                        Box::pin(fut)
                    }
                }
                let method = InsertDocumentsSvc(inner);
                let codec = tonic::codec::ProstCodec::default();
                let mut grpc = tonic::server::Grpc::new(codec);
                Box::pin(async move {
                    let res = grpc.unary(method, req).await;
                    Ok(res)
                })
            }
            "/faizdb.v1.FaizDbService/SubscribeChangeStream" => {
                struct SubscribeChangeStreamSvc<T: FaizDbService>(pub Arc<T>);
                impl<T: FaizDbService> tonic::server::ServerStreamingService<StreamRequest>
                    for SubscribeChangeStreamSvc<T>
                {
                    type Response = ChangeEventMsg;
                    type ResponseStream = T::SubscribeChangeStreamStream;
                    type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                    fn call(&mut self, request: tonic::Request<StreamRequest>) -> Self::Future {
                        let inner = self.0.clone();
                        let fut = async move { T::subscribe_change_stream(&inner, request).await };
                        Box::pin(fut)
                    }
                }
                let method = SubscribeChangeStreamSvc(inner);
                let codec = tonic::codec::ProstCodec::default();
                let mut grpc = tonic::server::Grpc::new(codec);
                Box::pin(async move {
                    let res = grpc.server_streaming(method, req).await;
                    Ok(res)
                })
            }
            "/faizdb.v1.FaizDbService/HealthCheck" => {
                struct HealthCheckSvc<T: FaizDbService>(pub Arc<T>);
                impl<T: FaizDbService> tonic::server::UnaryService<HealthRequest> for HealthCheckSvc<T> {
                    type Response = HealthResponse;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                    fn call(&mut self, request: tonic::Request<HealthRequest>) -> Self::Future {
                        let inner = self.0.clone();
                        let fut = async move { T::health_check(&inner, request).await };
                        Box::pin(fut)
                    }
                }
                let method = HealthCheckSvc(inner);
                let codec = tonic::codec::ProstCodec::default();
                let mut grpc = tonic::server::Grpc::new(codec);
                Box::pin(async move {
                    let res = grpc.unary(method, req).await;
                    Ok(res)
                })
            }
            _ => Box::pin(async move {
                let mut response = http::Response::new(tonic::body::BoxBody::default());
                let headers = response.headers_mut();
                headers.insert(
                    tonic::Status::GRPC_STATUS,
                    (tonic::Code::Unimplemented as i32).into(),
                );
                headers.insert(
                    http::header::CONTENT_TYPE,
                    tonic::metadata::GRPC_CONTENT_TYPE,
                );
                Ok(response)
            }),
        }
    }
}

impl<T: FaizDbService> Clone for FaizDbServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            accept_compression_encodings: self.accept_compression_encodings,
            send_compression_encodings: self.send_compression_encodings,
            max_decoding_message_size: self.max_decoding_message_size,
            max_encoding_message_size: self.max_encoding_message_size,
        }
    }
}

impl<T: FaizDbService> tonic::server::NamedService for FaizDbServiceServer<T> {
    const NAME: &'static str = "faizdb.v1.FaizDbService";
}
