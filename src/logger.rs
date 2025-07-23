use crate::{actor::Exit, global::send, receive};

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
    name: &'static str,
}

impl Logger {
    pub const fn global() -> Self {
        Self {
            name: "global_logger",
        }
    }

    pub fn debug(&self, msg: impl Into<String>) {
        send(self.name, Message::Debug(msg.into()));
    }
}
