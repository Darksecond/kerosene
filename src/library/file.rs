use std::{
    fs::File,
    io::{Error, Read},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Sender, channel},
    },
};

use crate::{
    Exit, global,
    port::{Port, PortContext},
    receive,
};

pub struct FilePort {
    path: PathBuf,
    tx: Option<Sender<FileRequest>>,
}

impl FilePort {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FilePort {
            path: path.into(),
            tx: None,
        }
    }
}

impl Port for FilePort {
    type Message = FileRequest;

    fn start(&mut self, ctx: &Arc<PortContext>) {
        let ctx = ctx.clone();
        let (tx, rx) = channel();
        self.tx = Some(tx);

        let path = self.path.clone();
        std::thread::spawn(move || {
            let mut file = match File::open(path) {
                Ok(file) => file,
                Err(err) => {
                    ctx.exit(Exit::Io(err.to_string(), err.kind()));
                    return;
                }
            };

            for msg in rx {
                match msg {
                    FileRequest::ReadString => {
                        let mut buffer = String::new();
                        match file.read_to_string(&mut buffer) {
                            Ok(_) => {
                                let _ = ctx.send(FileReply::ReadString(buffer));
                            }
                            Err(err) => {
                                ctx.exit(Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }
                    }
                }
            }
        });
    }

    fn stop(&mut self, _ctx: &Arc<PortContext>) {
        drop(self.tx.take());
    }

    fn receive(&mut self, _ctx: &Arc<PortContext>, message: Self::Message) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(message);
        }
    }
}

pub enum FileRequest {
    ReadString,
}

pub enum FileReply {
    ReadString(String),
}

// This should probably use an actor interally.
pub async fn read_string(path: impl Into<PathBuf>) -> Result<String, Error> {
    let port = global::create_port(FilePort::new(path));

    global::send_port(port, FileRequest::ReadString);

    receive! {
        match FileReply {
            FileReply::ReadString(string) => {
                global::close_port(port);
                return Ok(string)
            },
        }
    }
}
