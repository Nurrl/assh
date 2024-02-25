//! Session extension traits and helpers.

use futures::{AsyncBufRead, AsyncWrite, Future};

use crate::{session::Side, stream::Stream, Result};

#[cfg(doc)]
use crate::session::{client::Client, server::Server, Session};

/// An extension layer for a [`Session`].
///
/// A [`Layer`] can work either for both of the sides ([`Client`] and [`Server`])
/// or be constrained to a single [`Side`] using the type parameter.
///
/// In example, the no-op layer (`()`) can be used on both sides as seen there:
/// ```rust,no_run
/// # async fn test() -> Result<(), Box<dyn std::error::Error>> {
/// # use assh::session::{Session, client::Client, server::Server};
/// # let mut stream = futures::io::Cursor::new(Vec::<u8>::new());
/// Session::new(&mut stream, Client::default())
///     .await?
///     .add_layer(());
///
/// # let mut stream = futures::io::Cursor::new(Vec::<u8>::new());
/// Session::new(&mut stream, Server::default())
///     .await?
///     .add_layer(());
/// # Ok(()) }
/// ```
pub trait Layer<S: Side> {
    /// A method called _after successful key-exchange_.
    fn on_kex(
        &mut self,
        stream: &mut Stream<impl AsyncBufRead + AsyncWrite + Unpin>,
    ) -> impl Future<Output = Result<()>> {
        let _ = stream;

        async { Ok(()) }
    }

    /// A method called _before a message is received_.
    fn on_recv(
        &mut self,
        stream: &mut Stream<impl AsyncBufRead + AsyncWrite + Unpin>,
    ) -> impl Future<Output = Result<()>> {
        let _ = stream;

        async { Ok(()) }
    }
}

impl<S: Side> Layer<S> for () {}

impl<S: Side, A: Layer<S>, B: Layer<S>> Layer<S> for (A, B) {
    async fn on_kex(
        &mut self,
        stream: &mut Stream<impl AsyncBufRead + AsyncWrite + Unpin>,
    ) -> Result<()> {
        self.0.on_kex(stream).await?;
        self.1.on_kex(stream).await?;

        Ok(())
    }

    async fn on_recv(
        &mut self,
        stream: &mut Stream<impl AsyncBufRead + AsyncWrite + Unpin>,
    ) -> Result<()> {
        self.0.on_recv(stream).await?;
        self.1.on_recv(stream).await?;

        Ok(())
    }
}