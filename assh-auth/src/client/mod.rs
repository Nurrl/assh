//! Client-side authentication mechanics.

use std::collections::HashSet;

use assh::{
    layer::{Action, Layer},
    session::client::Client,
    stream::{Packet, Stream},
    Result,
};
use futures::{AsyncBufRead, AsyncWrite};

mod method;
use method::Method;

#[doc(no_inline)]
pub use ssh_key::PrivateKey;

#[derive(Debug, Default)]
enum State {
    #[default]
    Unauthorized,
    Authorized,
}

/// The authentication [`Layer`] for client-side sessions.
#[derive(Debug)]
pub struct Auth {
    state: State,

    username: String,
    methods: HashSet<Method>,
}

impl Auth {
    /// Create an [`Auth`] layer for the provided _username_.
    ///
    /// # Note
    /// The layer always starts with the `none` authentication method
    /// to discover the methods available on the server.
    ///
    /// Also while the `publickey` method allows for multiple tries,
    /// the `password` method will only keep the last one provided to [`Self::password`].
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            state: Default::default(),
            username: username.into(),
            methods: [Method::None].into_iter().collect(), // always attempt the `none` method
        }
    }

    /// Attempt to authenticate with the `password` method.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.methods.replace(Method::Password {
            password: password.into(),
        });

        self
    }

    /// Attempt to authenticate with the `publickey` method.
    pub fn publickey(mut self, key: impl Into<PrivateKey>) -> Self {
        self.methods.replace(Method::Publickey {
            key: Box::new(key.into()),
        });

        self
    }
}

impl Layer<Client> for Auth {
    async fn on_kex(
        &mut self,
        stream: &mut Stream<impl AsyncBufRead + AsyncWrite + Unpin + Send>,
    ) -> Result<()> {
        match self.state {
            State::Unauthorized => {
                //

                self.state = State::Authorized;
            }
            State::Authorized => {}
        }

        Ok(())
    }

    async fn on_recv(
        &mut self,
        _stream: &mut Stream<impl AsyncBufRead + AsyncWrite + Unpin + Send>,
        packet: Packet,
    ) -> Result<Action> {
        match self.state {
            State::Unauthorized => unreachable!("Authentication has not yet been performed, while `on_kex` should be called before."),
            State::Authorized => Ok(Action::Forward(packet)),
        }
    }
}
