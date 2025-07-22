use crate::{
    actor::{Exit, NamedRef},
    global::send,
    receive,
};

pub async fn logger_actor() -> Exit {
    loop {
        receive!({
            match Message: Message::Debug(msg) => {
                println!("[DEBUG] {}", msg);
            }
        });
    }
}

pub enum Message {
    Debug(String),
}

#[derive(Copy, Clone)]
pub struct Logger {
    inner: NamedRef,
}

impl Logger {
    pub const fn global() -> Self {
        Self {
            inner: NamedRef::new("global_logger"),
        }
    }

    pub fn debug(&self, msg: impl Into<String>) {
        send(self.inner, Message::Debug(msg.into()));
    }
}
