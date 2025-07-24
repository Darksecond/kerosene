use crate::{
    actor::{Exit, Pid, TrapExitMessage, TrapPortExitMessage},
    port::PortPid,
    receive,
};

pub trait IntoAsyncActor: Send + 'static {
    type Actor: Future<Output = Exit> + Send;

    fn into_async_actor(self) -> Self::Actor;
}

impl<T, F> IntoAsyncActor for T
where
    T: FnOnce() -> F + Send + 'static,
    F: Future<Output = Exit> + Send,
{
    type Actor = F;

    fn into_async_actor(self) -> Self::Actor {
        self()
    }
}

pub trait SimpleActor: Send + 'static + Sized {
    type Message: Send + 'static;

    fn started(&mut self) -> impl Future<Output = Option<Exit>> + Send {
        async move { None }
    }

    fn handle(&mut self, message: Self::Message) -> impl Future<Output = Option<Exit>> + Send;

    fn on_exit(&mut self, from: Pid, reason: Exit) -> impl Future<Output = Option<Exit>> + Send {
        let _ = from;
        async { Some(reason) }
    }

    fn on_port_exit(
        &mut self,
        from: PortPid,
        reason: Exit,
    ) -> impl Future<Output = Option<Exit>> + Send {
        let _ = from;
        async { Some(reason) }
    }
}

pub fn into_actor<A>(mut actor: A) -> impl IntoAsyncActor
where
    A: SimpleActor,
{
    async move || {
        if let Some(exit) = actor.started().await {
            return exit;
        }

        loop {
            receive!({
                match TrapExitMessage: TrapExitMessage { pid, reason } => {
                    if let Some(exit) = actor.on_exit(pid, reason).await
                    {
                        return exit;
                    }
                },
                match TrapPortExitMessage: TrapPortExitMessage { port, reason } => {
                    if let Some(exit) = actor.on_port_exit(port, reason).await
                    {
                        return exit;
                    }
                },
                match A::Message: message => {
                    if let Some(exit) = actor.handle(message).await {
                        return exit;
                    }
                }
            });
        }
    }
}
