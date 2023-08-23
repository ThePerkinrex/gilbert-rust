use std::{marker::PhantomData, sync::Arc};

use futures_util::{Sink, Stream};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::{
    client,
    rustls::{ClientConfig, ServerConnection, ServerName},
    server, TlsAcceptor, TlsConnector,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[pin_project]
#[derive(Debug)]
pub struct WebSocketByteStream<W> {
    #[pin]
    socket: W,
}

#[inline]
fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::default()
}

#[derive(Clone)]
pub struct Acceptor {
    tls: TlsAcceptor,
}

impl<T> From<T> for Acceptor
where
    T: Into<TlsAcceptor>,
{
    fn from(value: T) -> Self {
        Self { tls: value.into() }
    }
}

impl Acceptor {
    pub async fn accept<W: Send, I, O>(
        &self,
        ws: W,
    ) -> std::io::Result<DataStream<server::TlsStream<WebSocketByteStream<W>>, I, O>>
    where
        WebSocketByteStream<W>: AsyncRead + AsyncWrite + Unpin,
    {
        let stream = self.tls.accept(WebSocketByteStream { socket: ws }).await?;
        let framed = Framed::new(stream, codec());
        Ok(DataStream {
            inner: framed,
            msg: PhantomData,
        })
    }

    pub async fn accept_with<W: Send, I, O, F>(
        &self,
        ws: W,
        f: F,
    ) -> std::io::Result<DataStream<server::TlsStream<WebSocketByteStream<W>>, I, O>>
    where
        WebSocketByteStream<W>: AsyncRead + AsyncWrite + Unpin,
        F: FnOnce(&mut ServerConnection) + Send,
    {
        let stream = self
            .tls
            .accept_with(WebSocketByteStream { socket: ws }, f)
            .await?;
        let framed = Framed::new(stream, codec());
        Ok(DataStream {
            inner: framed,
            msg: PhantomData,
        })
    }

    pub async fn accept_with_server_name<W: Send, I, O>(
        &self,
        ws: W,
    ) -> std::io::Result<(
        DataStream<server::TlsStream<WebSocketByteStream<W>>, I, O>,
        Option<String>,
    )>
    where
        WebSocketByteStream<W>: AsyncRead + AsyncWrite + Unpin,
        I: Send,
        O: Send,
    {
        let stream = self.tls.accept(WebSocketByteStream { socket: ws }).await?;
        let mut i = 0;
        while i < 20 && stream.get_ref().1.server_name().is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            i += 1;
        }
        let name = stream.get_ref().1.server_name().map(ToString::to_string);
        let framed = Framed::new(stream, codec());
        Ok((
            DataStream {
                inner: framed,
                msg: PhantomData,
            },
            name,
        ))
    }
}

pub async fn connector<W: Send, I, O>(
    ws: W,
    domain: ServerName,
    config: Arc<ClientConfig>,
) -> std::io::Result<DataStream<client::TlsStream<WebSocketByteStream<W>>, I, O>>
where
    WebSocketByteStream<W>: AsyncRead + AsyncWrite + Unpin,
{
    let stream = TlsConnector::from(config)
        .connect(domain, WebSocketByteStream { socket: ws })
        .await?;
    let framed = Framed::new(stream, codec());
    Ok(DataStream {
        inner: framed,
        msg: PhantomData,
    })
}

#[pin_project]
pub struct DataStream<T, I, O = I> {
    #[pin]
    inner: Framed<T, LengthDelimitedCodec>,
    msg: PhantomData<(I, O)>,
}

#[derive(Debug, Error)]
pub enum DataStreamError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
}

impl<T, I, O> Stream for DataStream<T, I, O>
where
    T: AsyncRead + AsyncWrite,
    I: for<'a> Deserialize<'a>,
{
    type Item = Result<I, DataStreamError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx).map(|res| {
            res.map(|res| {
                res.map_err(Into::into)
                    .and_then(|bytes| bincode::deserialize(&bytes).map_err(Into::into))
            })
        })
    }
}

impl<T, I, O> Sink<O> for DataStream<T, I, O>
where
    T: AsyncRead + AsyncWrite,
    O: Serialize,
{
    type Error = DataStreamError;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx).map_err(Into::into)
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: O) -> Result<(), Self::Error> {
        let mut buf = Vec::with_capacity(bincode::serialized_size(&item)? as usize);
        bincode::serialize_into(&mut buf, &item)?;
        self.project().inner.start_send(buf.into())?;
        Ok(())
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx).map_err(Into::into)
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx).map_err(Into::into)
    }
}

pub mod axum_ws;
pub mod tungstenite;
