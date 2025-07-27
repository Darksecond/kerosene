use std::{
    fs::File,
    io::{Error, ErrorKind, Read},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Sender, channel},
    },
};

use crate::{
    global,
    port::{Port, PortContext},
    receive,
};

pub struct FilePort {
    tx: Option<Sender<FileRequest>>,
}

impl FilePort {
    pub fn new() -> Self {
        FilePort { tx: None }
    }
}

impl Port for FilePort {
    type Message = FileRequest;

    fn start(&mut self, ctx: &Arc<PortContext>) {
        let ctx = ctx.clone();
        let (tx, rx) = channel();
        self.tx = Some(tx);

        std::thread::spawn(move || {
            let mut file: Option<File> = None;

            for msg in rx {
                match msg {
                    FileRequest::Open { path } => match File::open(path) {
                        Ok(f) => {
                            file = Some(f);
                            let _ = ctx.send(FileReply::Opened);
                        }
                        Err(err) => {
                            file = None;
                            let _ = ctx.send(FileReply::Error(err));
                        }
                    },
                    FileRequest::Close => {
                        file = None;
                        let _ = ctx.send(FileReply::Closed);
                    }
                    FileRequest::ReadString => {
                        if let Some(file) = file.as_mut() {
                            let mut buffer = String::new();
                            match file.read_to_string(&mut buffer) {
                                Ok(_) => {
                                    let _ = ctx.send(FileReply::ReadString(buffer));
                                }
                                Err(err) => {
                                    let _ = ctx.send(FileReply::Error(err));
                                }
                            }
                        } else {
                            let _ = ctx.send(FileReply::Error(Error::new(
                                ErrorKind::Other,
                                "File not opened",
                            )));
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
    Open { path: PathBuf },
    Close,
    ReadString,
}

pub enum FileReply {
    Opened,
    Closed,
    Error(Error),
    ReadString(String),
}

// This should probably use an actor interally.
pub async fn read_string(path: impl Into<PathBuf>) -> Result<String, Error> {
    let path = path.into();
    let port = global::create_port(FilePort::new());

    global::send_port(port, FileRequest::Open { path });

    receive! {
        match FileReply {
            FileReply::Opened => {},
            FileReply::Error(error) => {
                return Err(error)
            }
        }
    }

    global::send_port(port, FileRequest::ReadString);

    receive! {
        match FileReply {
            FileReply::ReadString(string) => {
                global::close_port(port);
                return Ok(string)
            },
            FileReply::Error(error) => {
                global::close_port(port);
                return Err(error)
            }
        }
    }
}
