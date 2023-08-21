use std::{borrow::Cow, collections::HashMap, sync::Arc};

use chatter_protocol::ChatterMessage;
use futures_util::{stream::SplitSink, Sink, SinkExt, Stream, StreamExt};
use pin_project::pin_project;
use secure_comms::{DataStream, DataStreamError, WebSocketByteStream};
use tokio::{net::TcpStream, sync::RwLock, task::JoinHandle};
use tokio_tungstenite::MaybeTlsStream;

#[derive(Default)]
pub struct NodeManager {
    nodes: HashMap<Arc<str>, NodeStatus>,
}

impl NodeManager {
    pub fn get(&self, key: &str) -> Cow<NodeStatus> {
        self.nodes
            .get(key)
            .map_or(Cow::Owned(NodeStatus::Unknown), Cow::Borrowed)
    }

    pub fn down(&mut self, key: String) {
        self.nodes.insert(key.into(), NodeStatus::Down);
    }

    pub fn up(&mut self, key: String, connection: Connection) {
        self.nodes.insert(key.into(), NodeStatus::Up(connection));
    }
}

#[derive(Default, Clone)]
pub enum NodeStatus {
    Down,
    #[default]
    Unknown,
    Up(Connection),
}

type DataStreamShort<S, I, O = I> =
    DataStream<tokio_rustls::server::TlsStream<WebSocketByteStream<S>>, I, O>;
type SplitSinkDataStream<S, M> = SplitSink<DataStreamShort<S, M>, M>;

#[derive(Clone)]
pub struct Connection<M = ChatterMessage> {
    sink: Arc<RwLock<ConnectionSink<M>>>,
    handle: Arc<JoinHandle<()>>,
}

#[pin_project(project = ConnectionProj)]
pub enum ConnectionSink<M> {
    Accepted {
        #[pin]
        sink: SplitSinkDataStream<axum::extract::ws::WebSocket, M>,
    },
    Connected {
        #[pin]
        sink: SplitSinkDataStream<tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>, M>,
    },
}

impl Connection<ChatterMessage> {
    pub fn accepted(stream: DataStreamShort<axum::extract::ws::WebSocket, ChatterMessage>) -> Self {
        let (sink, stream) = stream.split();
        let sink = Arc::new(RwLock::new(ConnectionSink::Accepted { sink }));
        let handle = tokio::spawn(Self::receiver(stream, sink.clone()));
        Self {
            sink,
            handle: Arc::new(handle),
        }
    }

    pub fn connected(
        stream: DataStreamShort<
            tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>,
            ChatterMessage,
        >,
    ) -> Self {
        let (sink, stream) = stream.split();
        let sink = Arc::new(RwLock::new(ConnectionSink::Connected { sink }));
        let handle = tokio::spawn(Self::receiver(stream, sink.clone()));
        Self {
            sink,
            handle: Arc::new(handle),
        }
    }

    async fn receiver<
        St: Stream<Item = Result<ChatterMessage, DataStreamError>> + Send + Unpin,
        Si,
    >(
        mut stream: St,
        sink: Arc<RwLock<Si>>,
    ) where
		Si: Sink<ChatterMessage, Error = DataStreamError> + Unpin + Sync + Send,
    {
        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                ChatterMessage::QueueUpdate { length } => todo!(),
                ChatterMessage::NodeConfigUpdate { priority } => todo!(),
                ChatterMessage::GeneralConfigUpdate(_) => todo!(),
                ChatterMessage::Ping(x) => {
                    let _ = sink.write().await.send(ChatterMessage::Pong(x)).await;
                }
                ChatterMessage::Pong(_) => todo!(),
            }
        }
    }

    pub async fn send(&self, msg: ChatterMessage) -> Result<(), DataStreamError> {
        self.sink.write().await.send(msg).await
    }
}

impl<M> Sink<M> for ConnectionSink<M>
where
    M: serde::Serialize,
{
    type Error = DataStreamError;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        match self.project() {
            ConnectionProj::Accepted { sink, .. } => sink.poll_ready(cx),
            ConnectionProj::Connected { sink, .. } => sink.poll_ready(cx),
        }
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: M) -> Result<(), Self::Error> {
        match self.project() {
            ConnectionProj::Accepted { sink, .. } => sink.start_send(item),
            ConnectionProj::Connected { sink, .. } => sink.start_send(item),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        match self.project() {
            ConnectionProj::Accepted { sink, .. } => sink.poll_flush(cx),
            ConnectionProj::Connected { sink, .. } => sink.poll_flush(cx),
        }
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        match self.project() {
            ConnectionProj::Accepted { sink, .. } => sink.poll_close(cx),
            ConnectionProj::Connected { sink, .. } => sink.poll_close(cx),
        }
    }
}